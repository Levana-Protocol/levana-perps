use crate::state::funding::{LP_BORROW_FEE_DATA_SERIES, XLP_BORROW_FEE_DATA_SERIES};
use crate::state::*;

use anyhow::Context;
use cosmwasm_std::Decimal256;
use cw_storage_plus::Item;
use perpswap::contracts::market::deferred_execution::FeesReturnedEvent;
use perpswap::contracts::market::entry::Fees;
use perpswap::contracts::market::fees::events::{
    CrankFeeEarnedEvent, CrankFeeEvent, FeeEvent, FeeSource, InsufficientMarginEvent, TradeId,
};
use perpswap::contracts::market::position::PositionId;
use perpswap::prelude::*;

use self::liquidity::LiquidityNewYieldToProcess;

use super::funding::LpAndXlp;

/// Fees collected for LPs
pub(super) const ALL_FEES: Item<Fees> = Item::new(namespace::ALL_FEES);

pub(crate) fn all_fees(store: &dyn Storage) -> Result<Fees> {
    ALL_FEES.load(store).map_err(|err| err.into())
}

pub(crate) fn fees_init(store: &mut dyn Storage) -> Result<()> {
    ALL_FEES
        .save(
            store,
            &Fees {
                wallets: Collateral::zero(),
                protocol: Collateral::zero(),
                crank: Collateral::zero(),
                referral: Collateral::zero(),
            },
        )
        .map_err(anyhow::Error::from)?;

    Ok(())
}

impl State<'_> {
    // only earmarks the fee, doesn't transfer anything
    pub(crate) fn collect_borrow_fee(
        &self,
        store: &dyn Storage,
        pos_id: PositionId,
        amount: LpAndXlp,
        price: PricePoint,
    ) -> Result<BorrowFeeCollection> {
        let protocol_tax = self.config.protocol_tax;
        let protocol_fee_lp = amount.lp.checked_mul_dec(protocol_tax)?;
        let protocol_fee_xlp = amount.xlp.checked_mul_dec(protocol_tax)?;
        let lp_fee = amount.lp.checked_sub(protocol_fee_lp)?;
        let xlp_fee = amount.xlp.checked_sub(protocol_fee_xlp)?;
        let protocol_fee = protocol_fee_lp.checked_add(protocol_fee_xlp)?;
        debug_assert_eq!((protocol_fee + lp_fee)? + xlp_fee, amount.lp + amount.xlp);

        let liquidity_yield_to_process = self.liquidity_process_new_yield(
            store,
            LpAndXlp {
                lp: lp_fee,
                xlp: xlp_fee,
            },
        )?;

        Ok(BorrowFeeCollection {
            liquidity_yield_to_process,
            event: FeeEvent {
                trade_id: TradeId::Position(pos_id),
                fee_source: FeeSource::Borrow,
                lp_amount: lp_fee,
                lp_amount_usd: price.collateral_to_usd(lp_fee),
                xlp_amount: xlp_fee,
                xlp_amount_usd: price.collateral_to_usd(xlp_fee),
                protocol_amount: protocol_fee,
                protocol_amount_usd: price.collateral_to_usd(protocol_fee),
            },
        })
    }

    // only earmarks the fee, doesn't transfer anything
    fn collect_trading_fee_inner(
        &self,
        ctx: &mut StateContext,
        amount: Collateral,
        price: PricePoint,
        trade_id: TradeId,
        fee_source: FeeSource,
    ) -> Result<()> {
        let protocol_tax = self.config.protocol_tax;
        let protocol_fee = amount.checked_mul_dec(protocol_tax)?;
        let lp_and_xlp_fee = amount.checked_sub(protocol_fee)?;
        debug_assert_eq!(protocol_fee + lp_and_xlp_fee, Ok(amount));

        // Use the current ratio of LP to xLP rewards to split up the trading fee
        // We can assert that there is at least some liquidity in the system
        // because all events that pay trading fees requiring liquidity.
        let lp = LP_BORROW_FEE_DATA_SERIES
            .try_load_last(ctx.storage)?
            .map_or(Number::ZERO, |x| x.1.value)
            .try_into_non_negative_value()
            .context("LP_BORROW_FEE_DATA_SERIES gave a negative value")?;
        let xlp = XLP_BORROW_FEE_DATA_SERIES
            .try_load_last(ctx.storage)?
            .map_or(Number::ZERO, |x| x.1.value)
            .try_into_non_negative_value()
            .context("XLP_BORROW_FEE_DATA_SERIES gave a negative value")?;
        anyhow::ensure!(
            !lp.is_zero() || !xlp.is_zero(),
            "Cannot receive a trading fee if there is no liquidity in the system"
        );

        // To avoid rounding errors, explicitly deal with the zero case
        let (lp_fee, xlp_fee) = if lp.is_zero() {
            (Collateral::zero(), lp_and_xlp_fee)
        } else if xlp.is_zero() {
            (lp_and_xlp_fee, Collateral::zero())
        } else {
            let lp_fee = Collateral::from_decimal256(
                lp_and_xlp_fee
                    .into_decimal256()
                    .checked_mul(lp)?
                    .checked_div(lp.checked_add(xlp)?)?,
            );
            let xlp_fee = lp_and_xlp_fee.checked_sub(lp_fee)?;
            (lp_fee, xlp_fee)
        };

        ALL_FEES.update(ctx.storage, |mut fee| {
            fee.wallets = (fee.wallets + (lp_fee + xlp_fee)?)?;
            fee.protocol = (fee.protocol + protocol_fee)?;

            anyhow::Ok(fee)
        })?;

        self.liquidity_process_new_yield(
            ctx.storage,
            LpAndXlp {
                lp: lp_fee,
                xlp: xlp_fee,
            },
        )?
        .apply(self, ctx)?;

        ctx.response_mut().add_event(FeeEvent {
            trade_id,
            fee_source,
            lp_amount: lp_fee,
            lp_amount_usd: price.collateral_to_usd(lp_fee),
            xlp_amount: xlp_fee,
            xlp_amount_usd: price.collateral_to_usd(xlp_fee),
            protocol_amount: protocol_fee,
            protocol_amount_usd: price.collateral_to_usd(protocol_fee),
        });

        Ok(())
    }

    pub(crate) fn collect_trading_fee(
        &self,
        ctx: &mut StateContext,
        pos_id: PositionId,
        amount: Collateral,
        price: PricePoint,
        fee_source: FeeSource,
        owner: &Addr,
    ) -> Result<()> {
        let amount = match self.get_referrer_for(owner)? {
            None => amount,
            Some(referrer) => {
                let reward = amount.checked_mul_dec(self.config.referral_reward_ratio)?;

                let mut fees = ALL_FEES.load(ctx.storage)?;
                fees.referral = fees.referral.checked_add(reward)?;
                ALL_FEES.save(ctx.storage, &fees)?;

                let mut addr_stats =
                    self.load_liquidity_stats_addr_default(ctx.storage, &referrer)?;
                addr_stats.referrer_rewards = addr_stats.referrer_rewards.checked_add(reward)?;
                self.save_liquidity_stats_addr(ctx.storage, &referrer, &addr_stats)?;

                if let Some(reward) = NonZero::new(reward) {
                    self.add_summary_referral(ctx, owner, &referrer, reward)?;
                }

                (amount - reward)?
            }
        };
        self.collect_trading_fee_inner(ctx, amount, price, TradeId::Position(pos_id), fee_source)?;

        Ok(())
    }

    pub(crate) fn collect_delta_neutrality_fee_for_protocol(
        &self,
        ctx: &mut StateContext,
        pos_id: PositionId,
        amount: Collateral,
        price: PricePoint,
    ) -> Result<()> {
        self.collect_trading_fee_inner(
            ctx,
            amount,
            price,
            TradeId::Position(pos_id),
            FeeSource::DeltaNeutrality,
        )?;

        Ok(())
    }

    pub(crate) fn register_lp_claimed_yield(
        &self,
        ctx: &mut StateContext,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        ALL_FEES.update(ctx.storage, |mut fee| {
            fee.wallets = fee.wallets.checked_sub(amount.raw())?;
            anyhow::Ok(fee)
        })?;

        Ok(())
    }

    pub(crate) fn transfer_fees_to_dao(&self, ctx: &mut StateContext) -> Result<()> {
        let mut fees_before = ALL_FEES
            .may_load(ctx.storage)?
            .context("ALL_FEES is empty")?;
        let amount =
            NonZero::new(fees_before.protocol).context("No DAO fees available to transfer")?;
        fees_before.protocol = Collateral::zero();
        ALL_FEES.save(ctx.storage, &fees_before)?;

        let dao_addr: Addr = load_external_item(
            &self.querier,
            &self.factory_address,
            namespace::DAO_ADDR.as_bytes(),
        )?;

        self.add_token_transfer_msg(ctx, &dao_addr, amount)?;

        Ok(())
    }

    /// Increase the crank fee balance
    pub(crate) fn provide_crank_funds(
        &self,
        ctx: &mut StateContext,
        added: NonZero<Collateral>,
    ) -> Result<()> {
        ALL_FEES.update(ctx.storage, |mut fees| {
            fees.crank = fees.crank.checked_add(added.raw())?;
            anyhow::Ok(fees)
        })?;
        Ok(())
    }

    /// Allocate crank fees to the given wallet
    ///
    /// This is on a "best effort" basis. If there are insufficient funds
    /// available, the wallet simply does not receive the fees.
    pub(crate) fn allocate_crank_fees(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        cranks: u32,
    ) -> Result<()> {
        if cranks == 0 {
            return Ok(());
        }

        let max_payments = Collateral::from_decimal256(
            self.config
                .crank_fee_reward
                .into_decimal256()
                .checked_mul(Decimal256::from_atomics(cranks, 0)?)?,
        );
        let mut fees = ALL_FEES.load(ctx.storage)?;
        let payment = max_payments.min(fees.crank);
        if let Some(payment) = NonZero::new(payment) {
            fees.crank = fees.crank.checked_sub(payment.raw())?;
            fees.wallets = fees.wallets.checked_add(payment.raw())?;
            ALL_FEES.save(ctx.storage, &fees)?;
            self.add_lp_crank_rewards(ctx, addr, payment)?;

            let price_point = self.current_spot_price(ctx.storage)?;
            ctx.response_mut().add_event(CrankFeeEarnedEvent {
                recipient: addr.clone(),
                amount: payment,
                amount_usd: price_point.collateral_to_usd_non_zero(payment),
            });
        }
        Ok(())
    }

    /// Returns funds to a user as part of the rewards system.
    ///
    /// Used when over-paying crank fees
    pub(crate) fn return_funds_to_user(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        amount: NonZero<Collateral>,
        price_point: &PricePoint,
    ) -> Result<()> {
        let mut fees = ALL_FEES.load(ctx.storage)?;
        fees.wallets = fees.wallets.checked_add(amount.raw())?;
        ALL_FEES.save(ctx.storage, &fees)?;
        self.add_lp_crank_rewards(ctx, addr, amount)?;
        ctx.response_mut().add_event(FeesReturnedEvent {
            recipient: addr.clone(),
            amount,
            amount_usd: price_point.collateral_to_usd_non_zero(amount),
        });
        Ok(())
    }
}

#[must_use]
pub(crate) struct BorrowFeeCollection {
    pub(crate) event: FeeEvent,
    pub(crate) liquidity_yield_to_process: LiquidityNewYieldToProcess,
}

impl BorrowFeeCollection {
    pub(crate) fn apply(self, state: &State, ctx: &mut StateContext) -> Result<()> {
        let Self {
            event,
            liquidity_yield_to_process,
        } = self;

        liquidity_yield_to_process.apply(state, ctx)?;

        ALL_FEES.update::<_, anyhow::Error>(ctx.storage, |mut fee| {
            fee.wallets = (fee.wallets + (event.lp_amount + event.xlp_amount)?)?;
            fee.protocol = (fee.protocol + event.protocol_amount)?;
            Ok(fee)
        })?;

        ctx.response_mut().add_event(event.clone());
        Ok(())
    }
}

#[must_use]
pub(crate) struct CapCrankFee {
    pub(crate) amount: Collateral,
    pub(crate) amount_usd: Usd,
    pub(crate) insufficient_margin_event: Option<InsufficientMarginEvent>,
    pub(crate) trade_id: TradeId,
}

impl CapCrankFee {
    pub(crate) fn new(amount: Collateral, amount_usd: Usd, trade_id: TradeId) -> Self {
        Self {
            amount,
            amount_usd,
            insufficient_margin_event: None,
            trade_id,
        }
    }

    pub(crate) fn apply(self, _state: &State, ctx: &mut StateContext) -> Result<()> {
        let Self {
            trade_id,
            amount,
            amount_usd,
            insufficient_margin_event,
        } = self;
        if let Some(event) = insufficient_margin_event {
            ctx.response_mut().add_event(event);
        }

        let mut fees = ALL_FEES.load(ctx.storage)?;
        let old_balance = fees.crank;
        fees.crank = fees.crank.checked_add(amount)?;
        ALL_FEES.save(ctx.storage, &fees)?;

        ctx.response_mut().add_event(CrankFeeEvent {
            trade_id: trade_id.clone(),
            amount,
            amount_usd,
            old_balance,
            new_balance: fees.crank,
        });

        Ok(())
    }
}
