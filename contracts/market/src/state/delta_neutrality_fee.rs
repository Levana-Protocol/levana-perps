use super::{State, StateContext};
use crate::prelude::*;
use cosmwasm_std::{Decimal256, Storage};
use cw_storage_plus::Item;
use msg::contracts::market::{
    config::Config,
    delta_neutrality_fee::{DeltaNeutralityFeeEvent, DeltaNeutralityFeeReason},
    fees::events::{FeeType, InsufficientMarginEvent},
    position::Position,
};

pub(crate) const DELTA_NEUTRALITY_FUND: Item<Collateral> =
    Item::new(namespace::DELTA_NEUTRALITY_FUND);

impl State<'_> {
    pub(crate) fn calc_delta_neutrality_fee(
        &self,
        store: &dyn Storage,
        notional_delta: Signed<Notional>,
        price: PricePoint,
        liquidation_margin_factor: Option<Collateral>,
    ) -> Result<Signed<Collateral>> {
        let mut calc = DeltaNeutralityFeeMultiPass::new(
            store,
            self.config.clone(),
            self.positions_net_open_interest(store)?,
            notional_delta,
            price,
            liquidation_margin_factor,
        )?;

        calc.run()?;
        Ok(calc.fees)
    }

    pub(crate) fn charge_delta_neutrality_fee(
        &self,
        store: &dyn Storage,
        pos: &mut Position,
        notional_delta: Signed<Notional>,
        price: PricePoint,
        reason: DeltaNeutralityFeeReason,
    ) -> Result<ChargeDeltaNeutralityFeeResult> {
        let res =
            self.charge_delta_neutrality_fee_no_update(store, pos, notional_delta, price, reason)?;
        pos.active_collateral = pos.active_collateral.checked_sub_signed(res.fee)?;
        pos.add_delta_neutrality_fee(res.fee, &price)?;
        Ok(res)
    }

    /// Same as [State::charge_delta_neutrality_fee], but doesn't update the
    /// active_collateral or cumulative delta neutrality values on the position.
    pub(crate) fn charge_delta_neutrality_fee_no_update(
        &self,
        store: &dyn Storage,
        pos: &Position,
        notional_delta: Signed<Notional>,
        price: PricePoint,
        reason: DeltaNeutralityFeeReason,
    ) -> Result<ChargeDeltaNeutralityFeeResult> {
        let mut calc = DeltaNeutralityFeeMultiPass::new(
            store,
            self.config.clone(),
            self.positions_net_open_interest(store)?,
            notional_delta,
            price,
            match reason {
                DeltaNeutralityFeeReason::PositionClose
                | DeltaNeutralityFeeReason::PositionUpdate => {
                    Some(pos.liquidation_margin.delta_neutrality)
                }
                DeltaNeutralityFeeReason::PositionOpen => None,
            },
        )?;

        calc.run()?;

        // If this is a payment into the fund, take some of the fees for the protocol
        let protocol_fees = (|| {
            let fees = calc.fees.try_into_non_zero()?;
            let protocol_fees = fees
                .raw()
                .checked_mul_dec(self.config.delta_neutrality_fee_tax)
                .ok()?;
            Some(protocol_fees)
        })()
        .unwrap_or_default();
        let fund_fees = calc.fees.checked_sub(protocol_fees.into_signed())?;

        let total_funds_after = calc
            .total_in_fund_before_calc
            .checked_add_signed(fund_fees)?;

        Ok(ChargeDeltaNeutralityFeeResult {
            pos_id: pos.id,
            fee: calc.fees,
            fee_event: DeltaNeutralityFeeEvent {
                amount: fund_fees,
                total_funds_before: calc.total_in_fund_before_calc,
                total_funds_after,
                reason,
                protocol_amount: protocol_fees,
            },
            cap_triggered_info: calc.cap_triggered_info,
            total_funds_after,
            protocol_fees,
            price,
        })
    }
}

/// Result of charging a delta neutrality fee.
///
/// This struct keeps track of the results of successfully calculating a delta
/// neutrality fee to be charged. We do this in a two-step process
/// (calculate/validate and then save) so that, when opening limit orders, we
/// can fully validate a position before writing anything to storage.
#[must_use]
pub(crate) struct ChargeDeltaNeutralityFeeResult {
    pos_id: PositionId,
    fee: Signed<Collateral>,
    fee_event: DeltaNeutralityFeeEvent,
    cap_triggered_info: Option<CapTriggeredInfo>,
    total_funds_after: Collateral,
    protocol_fees: Collateral,
    price: PricePoint,
}

impl ChargeDeltaNeutralityFeeResult {
    /// Consume this value and write its data to storage.
    pub(crate) fn store(self, state: &State, ctx: &mut StateContext) -> Result<Signed<Collateral>> {
        let ChargeDeltaNeutralityFeeResult {
            pos_id,
            fee,
            fee_event,
            cap_triggered_info,
            total_funds_after,
            protocol_fees,
            price,
        } = self;
        ctx.response_mut().add_event(fee_event);

        if let Some(CapTriggeredInfo {
            available,
            requested,
        }) = cap_triggered_info
        {
            ctx.response_mut().add_event(InsufficientMarginEvent {
                pos: pos_id,
                fee_type: FeeType::DeltaNeutrality,
                available: available.into_signed(),
                requested: requested.into_signed(),
                desc: None,
            });
        }

        DELTA_NEUTRALITY_FUND.save(ctx.storage, &total_funds_after)?;

        state.collect_delta_neutrality_fee_for_protocol(ctx, pos_id, protocol_fees, price)?;
        Ok(fee)
    }
}

// the delta neutrality fee is done in multiple passes sometimes
// so it's all encapsulated in this struct
#[derive(Debug)]
struct DeltaNeutralityFeeMultiPass {
    fees: Signed<Collateral>,
    total_in_fund_before_calc: Collateral,
    config: Config,
    net_notional: Signed<Notional>,
    delta_notional: Signed<Notional>,
    price: PricePoint,
    liquidation_margin_factor: Option<Collateral>,
    cap_triggered_info: Option<CapTriggeredInfo>,
}

#[derive(PartialEq, Eq, Debug)]
struct CapTriggeredInfo {
    available: Collateral,
    requested: NonZero<Collateral>,
}

impl DeltaNeutralityFeeMultiPass {
    pub fn new(
        store: &dyn Storage,
        config: Config,
        net_notional: Signed<Notional>,
        delta_notional: Signed<Notional>,
        price: PricePoint,
        liquidation_margin_factor: Option<Collateral>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            net_notional,
            delta_notional,
            fees: Signed::<Collateral>::zero(),
            total_in_fund_before_calc: DELTA_NEUTRALITY_FUND.may_load(store)?.unwrap_or_default(),
            price,
            liquidation_margin_factor,
            cap_triggered_info: None,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let net_notional_after = self.net_notional + self.delta_notional;
        let net_notional = self.net_notional;
        if (net_notional.into_number() * net_notional_after.into_number()).is_negative() {
            self.update_inner(-net_notional)?;
            self.update_inner(self.delta_notional + net_notional)?;
        } else {
            self.update_inner(self.delta_notional)?;
        }

        Ok(())
    }

    fn update_inner(&mut self, delta_notional: Signed<Notional>) -> Result<()> {
        let total_in_fund_so_far = self
            .total_in_fund_before_calc
            .checked_add_signed(self.fees)?;

        let amount_in_notional = calculate_delta_neutrality_fee_amount(
            self.config.delta_neutrality_fee_cap.into(),
            self.config.delta_neutrality_fee_sensitivity.into(),
            self.net_notional.into_number(),
            delta_notional.into_number(),
        )?;

        let amount_in_notional = Signed::<Notional>::from_number(amount_in_notional);
        let mut amount_in_collateral =
            amount_in_notional.map(|x| self.price.notional_to_collateral(x));

        if amount_in_collateral.is_negative() {
            let slippage_to_balance_net_notional_in_notional = Notional::from_decimal256(
                calculate_delta_neutrality_fee_amount(
                    self.config.delta_neutrality_fee_cap.into(),
                    self.config.delta_neutrality_fee_sensitivity.into(),
                    self.net_notional.into_number(),
                    -self.net_notional.into_number(),
                )?
                .abs_unsigned(),
            );
            let slippage_to_balance_net_notional_in_collateral = self
                .price
                .notional_to_collateral(slippage_to_balance_net_notional_in_notional);

            let slippage_fundedness_ratio =
                match NonZero::new(slippage_to_balance_net_notional_in_collateral) {
                    Some(slippage_to_balance_net_notional_in_collateral)
                        if !slippage_to_balance_net_notional_in_collateral
                            .raw()
                            .approx_eq(Collateral::zero()) =>
                    {
                        total_in_fund_so_far
                            .div_non_zero(slippage_to_balance_net_notional_in_collateral)
                    }
                    _ => Decimal256::one(),
                };

            // Don't allow traders to take too much from the fund. When paying
            // out, cap the ratio at 1.
            let slippage_fundedness_ratio = slippage_fundedness_ratio.min(Decimal256::one());

            let n =
                amount_in_collateral.checked_mul_number(slippage_fundedness_ratio.into_signed())?;

            // Due to rounding errors, it's possible for the calculated
            // amount_in_collateral to be slightly greater than the amount in the
            // fund. If that's the case, cap it.
            amount_in_collateral = if -n > total_in_fund_so_far.into_signed() {
                // This should just be a rounding error, so prove that with a debug assertion
                debug_assert!((-n)
                    .into_number()
                    .approx_eq(total_in_fund_so_far.into_number()));
                -total_in_fund_so_far.into_signed()
            } else {
                n
            };
        }

        let amount_in_collateral = match self.liquidation_margin_factor {
            Some(cap) => {
                match amount_in_collateral.try_into_non_zero() {
                    Some(amount_in_collateral) if amount_in_collateral.raw() > cap => {
                        // Can only use the cap once
                        debug_assert_eq!(self.cap_triggered_info, None);
                        // Need to cap the amount, emit an event about insufficient margin
                        self.cap_triggered_info = Some(CapTriggeredInfo {
                            available: cap,
                            requested: amount_in_collateral,
                        });
                        cap.into_signed()
                    }
                    _ => amount_in_collateral,
                }
            }
            None => amount_in_collateral,
        };

        self.fees += amount_in_collateral;

        debug_assert!(
            (self.total_in_fund_before_calc.into_signed() + self.fees).is_positive_or_zero()
        );

        self.net_notional += delta_notional;

        Ok(())
    }
}

fn calculate_delta_neutrality_fee_amount(
    cap: Number,
    sensitivity: Number,
    net_notional: Number,
    delta_notional: Number,
) -> Result<Number> {
    let notional_low_cap = -cap * sensitivity;
    let notional_high_cap = cap * sensitivity;

    let delta_notional_at_low_cap =
        (net_notional + delta_notional).min(notional_low_cap) - net_notional.min(notional_low_cap);
    let delta_notional_at_high_cap = (net_notional + delta_notional).max(notional_high_cap)
        - net_notional.max(notional_high_cap);
    let delta_notional_uncapped =
        delta_notional - delta_notional_at_low_cap - delta_notional_at_high_cap;

    let mut delta_neutrality_fee_amount = delta_notional_at_low_cap * -cap;
    delta_neutrality_fee_amount += delta_notional_at_high_cap * cap;
    delta_neutrality_fee_amount += (delta_notional_uncapped * delta_notional_uncapped
        + Number::two()
            * delta_notional_uncapped
            * net_notional.min(notional_high_cap).max(notional_low_cap))
        / (Number::two() * sensitivity);

    Ok(delta_neutrality_fee_amount)
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn delta_neutrality_fee_cumulative() {
        let cap = Number::from_str("0.05").unwrap();
        let sensitivity = Number::from_str("1000000").unwrap();
        let net_notional = Number::from_str("119000").unwrap();
        let delta = Number::from_str("-95000").unwrap();

        let fee1 =
            calculate_delta_neutrality_fee_amount(cap, sensitivity, net_notional, delta).unwrap();
        let fee2 = calculate_delta_neutrality_fee_amount(
            cap,
            sensitivity,
            net_notional + delta,
            -(net_notional + delta),
        )
        .unwrap();
        let fee3 =
            calculate_delta_neutrality_fee_amount(cap, sensitivity, net_notional, -net_notional)
                .unwrap();

        assert_eq!(fee1 + fee2, fee3)
    }

    proptest! {
        #[test]
        #[cfg_attr(not(feature = "proptest"), ignore)]
        fn delta_neutrality_fee_amount(
            cap in 0.0f32..1.0,
            sensitivity in 1.0f32..10.0,
            market_notional_net in -10.0f32..10.0,
            pos_notional_delta in -10.0f32..10.0,
        ) {
            let _ = calculate_delta_neutrality_fee_amount(
                Number::try_from(cap.to_string()).unwrap(),
                Number::try_from(sensitivity.to_string()).unwrap(),
                Number::try_from(market_notional_net.to_string()).unwrap(),
                Number::try_from(pos_notional_delta.to_string()).unwrap(),
            ).unwrap();
        }
    }
}
