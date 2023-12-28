//! Events and helper methods for fees.
use shared::prelude::*;

use super::config::Config;

impl Config {
    /// Calculate the trade fee based on the given old and new position parameters.
    ///
    /// When opening a new position, you can use [Config::calculate_trade_fee_open].
    pub fn calculate_trade_fee(
        &self,
        old_notional_size_in_collateral: Signed<Collateral>,
        new_notional_size_in_collateral: Signed<Collateral>,
        old_counter_collateral: Collateral,
        new_counter_collateral: Collateral,
    ) -> Result<Collateral> {
        debug_assert!(
            old_notional_size_in_collateral.is_zero()
                || (old_notional_size_in_collateral.is_negative()
                    == new_notional_size_in_collateral.is_negative())
        );
        let old_notional_size_in_collateral = old_notional_size_in_collateral.abs_unsigned();
        let new_notional_size_in_collateral = new_notional_size_in_collateral.abs_unsigned();
        let notional_size_fee = match new_notional_size_in_collateral
            .checked_sub(old_notional_size_in_collateral)
            .ok()
        {
            Some(delta) => {
                debug_assert!(old_notional_size_in_collateral <= new_notional_size_in_collateral);
                delta.checked_mul_dec(self.trading_fee_notional_size)?
            }
            None => {
                debug_assert!(old_notional_size_in_collateral > new_notional_size_in_collateral);
                Collateral::zero()
            }
        };
        let counter_collateral_fee = match new_counter_collateral
            .checked_sub(old_counter_collateral)
            .ok()
        {
            Some(delta) => {
                debug_assert!(old_counter_collateral <= new_counter_collateral);
                delta.checked_mul_dec(self.trading_fee_counter_collateral)?
            }
            None => {
                debug_assert!(old_counter_collateral > new_counter_collateral);
                Collateral::zero()
            }
        };
        notional_size_fee
            .checked_add(counter_collateral_fee)
            .context("Overflow when calculating trading fee")
    }

    /// Same as [Config::calculate_trade_fee] but for opening a new position.
    pub fn calculate_trade_fee_open(
        &self,
        notional_size_in_collateral: Signed<Collateral>,
        counter_collateral: Collateral,
    ) -> Result<Collateral> {
        self.calculate_trade_fee(
            Signed::zero(),
            notional_size_in_collateral,
            Collateral::zero(),
            counter_collateral,
        )
    }
}

/// Events for fees.
pub mod events {
    use super::*;
    use crate::constants::event_key;
    use crate::contracts::market::order::OrderId;
    use crate::contracts::market::position::PositionId;
    use cosmwasm_std::{Decimal256, Event};

    /// Represents either a [PositionId] or an [OrderId]
    #[derive(Debug, Clone)]
    pub enum TradeId {
        /// An open position
        Position(PositionId),
        /// A pending limit order
        LimitOrder(OrderId),
    }

    /// The type of fee that was paid out
    #[derive(Debug, Clone, Copy)]
    pub enum FeeSource {
        /// Trading fees
        Trading,
        /// Borrow fees
        Borrow,
        /// Delta neutrality fee
        DeltaNeutrality,
    }

    impl FeeSource {
        fn as_str(self) -> &'static str {
            match self {
                FeeSource::Trading => "trading",
                FeeSource::Borrow => "borrow",
                FeeSource::DeltaNeutrality => "delta-neutrality",
            }
        }
    }

    impl FromStr for FeeSource {
        type Err = anyhow::Error;

        fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
            match s {
                "trading" => Ok(FeeSource::Trading),
                "borrow" => Ok(FeeSource::Borrow),
                "delta-neutrality" => Ok(FeeSource::DeltaNeutrality),
                _ => Err(anyhow::anyhow!("Unknown FeeSource {s}")),
            }
        }
    }

    /// Event fired whenever a fee is collected
    #[derive(Debug, Clone)]
    pub struct FeeEvent {
        /// Position that triggered the fee
        pub trade_id: TradeId,
        /// Source of the fee
        pub fee_source: FeeSource,
        /// Amount paid to LP holders, in collateral
        pub lp_amount: Collateral,
        /// Amount paid to LP holders, in USD
        pub lp_amount_usd: Usd,
        /// Amount paid to xLP holders, in collateral
        pub xlp_amount: Collateral,
        /// Amount paid to xLP holders, in USD
        pub xlp_amount_usd: Usd,
        /// Amount paid to the protocol/DAO, in collateral
        pub protocol_amount: Collateral,
        /// Amount paid to the protocol/DAO, in USD
        pub protocol_amount_usd: Usd,
    }

    impl PerpEvent for FeeEvent {}
    impl From<FeeEvent> for Event {
        fn from(
            FeeEvent {
                trade_id,
                fee_source,
                lp_amount,
                lp_amount_usd,
                xlp_amount,
                xlp_amount_usd,
                protocol_amount,
                protocol_amount_usd,
            }: FeeEvent,
        ) -> Self {
            let (trade_id_key, trade_id_val) = match trade_id {
                TradeId::Position(pos_id) => ("pos-id", pos_id.to_string()),
                TradeId::LimitOrder(order_id) => ("order-id", order_id.to_string()),
            };

            Event::new("fee")
                .add_attribute(trade_id_key, trade_id_val)
                .add_attribute("source", fee_source.as_str())
                .add_attribute("lp-amount", lp_amount.to_string())
                .add_attribute("lp-amount-usd", lp_amount_usd.to_string())
                .add_attribute("xlp-amount", xlp_amount.to_string())
                .add_attribute("xlp-amount-usd", xlp_amount_usd.to_string())
                .add_attribute("protocol-amount", protocol_amount.to_string())
                .add_attribute("protocol-amount-usd", protocol_amount_usd.to_string())
        }
    }
    impl TryFrom<Event> for FeeEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            let trade_id = match evt.try_u64_attr("pos-id")? {
                Some(pos_id) => TradeId::Position(PositionId::new(pos_id)),
                None => {
                    let order_id = evt.u64_attr("order-id")?;
                    TradeId::LimitOrder(OrderId::new(order_id))
                }
            };

            Ok(FeeEvent {
                trade_id,
                fee_source: evt.string_attr("source")?.parse()?,
                lp_amount: evt.decimal_attr("lp-amount")?,
                lp_amount_usd: evt.decimal_attr("lp-amount-usd")?,
                xlp_amount: evt.decimal_attr("xlp-amount")?,
                xlp_amount_usd: evt.decimal_attr("xlp-amount-usd")?,
                protocol_amount: evt.decimal_attr("protocol-amount")?,
                protocol_amount_usd: evt.decimal_attr("protocol-amount-usd")?,
            })
        }
    }

    /// Event when a funding payment is made
    pub struct FundingPaymentEvent {
        /// Position that paid (or received) the payment
        pub pos_id: PositionId,
        /// Size of the payment, negative means paid to the posiiton
        pub amount: Signed<Collateral>,
        /// Amount expressed in USD
        pub amount_usd: Signed<Usd>,
        /// Whether the position is long or short
        pub direction: DirectionToBase,
    }

    impl PerpEvent for FundingPaymentEvent {}
    impl From<FundingPaymentEvent> for Event {
        fn from(src: FundingPaymentEvent) -> Self {
            Event::new("funding-payment")
                .add_attribute("pos-id", src.pos_id.to_string())
                .add_attribute("amount", src.amount.to_string())
                .add_attribute("amount-usd", src.amount_usd.to_string())
                .add_attribute(event_key::DIRECTION, src.direction.as_str())
        }
    }

    impl TryFrom<Event> for FundingPaymentEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(FundingPaymentEvent {
                pos_id: PositionId::new(evt.u64_attr("pos-id")?),
                amount: evt.number_attr("amount")?,
                amount_usd: evt.number_attr("amount-usd")?,
                direction: evt.direction_attr(event_key::DIRECTION)?,
            })
        }
    }

    /// The funding rate was changed
    pub struct FundingRateChangeEvent {
        /// When the change happened
        pub time: Timestamp,
        /// Long is in terms of base, not notional
        pub long_rate_base: Number,
        /// Short is in terms of base, not notional
        pub short_rate_base: Number,
    }

    impl PerpEvent for FundingRateChangeEvent {}
    impl From<FundingRateChangeEvent> for Event {
        fn from(
            FundingRateChangeEvent {
                time,
                long_rate_base,
                short_rate_base,
            }: FundingRateChangeEvent,
        ) -> Self {
            Event::new("funding-rate-change")
                .add_attribute("time", time.to_string())
                .add_attribute("long-rate", long_rate_base.to_string())
                .add_attribute("short-rate", short_rate_base.to_string())
        }
    }

    impl TryFrom<Event> for FundingRateChangeEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(FundingRateChangeEvent {
                time: evt.timestamp_attr("time")?,
                long_rate_base: evt.number_attr("long-rate-base")?,
                short_rate_base: evt.number_attr("short-rate-base")?,
            })
        }
    }

    /// The borrow fee was changed
    pub struct BorrowFeeChangeEvent {
        /// When it was changed
        pub time: Timestamp,
        /// Sum of LP and xLP rate
        pub total_rate: Decimal256,
        /// Amount paid to LP holders
        pub lp_rate: Decimal256,
        /// Amount paid to xLP holders
        pub xlp_rate: Decimal256,
    }

    impl PerpEvent for BorrowFeeChangeEvent {}

    impl From<BorrowFeeChangeEvent> for Event {
        fn from(
            BorrowFeeChangeEvent {
                time,
                total_rate,
                lp_rate,
                xlp_rate,
            }: BorrowFeeChangeEvent,
        ) -> Self {
            Event::new("borrow-fee-change")
                .add_attribute("time", time.to_string())
                .add_attribute("total", total_rate.to_string())
                .add_attribute("lp", lp_rate.to_string())
                .add_attribute("xlp", xlp_rate.to_string())
        }
    }

    impl TryFrom<Event> for BorrowFeeChangeEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> Result<Self, Self::Error> {
            Ok(BorrowFeeChangeEvent {
                time: evt.timestamp_attr("time")?,
                total_rate: evt.decimal_attr("total")?,
                lp_rate: evt.decimal_attr("lp")?,
                xlp_rate: evt.decimal_attr("xlp")?,
            })
        }
    }

    /// A crank fee was collected
    pub struct CrankFeeEvent {
        /// Position that paid the fee
        pub trade_id: TradeId,
        /// Amount paid, in collateral
        pub amount: Collateral,
        /// Amount paid, in USD
        pub amount_usd: Usd,
        /// Old crank fee fund balance
        pub old_balance: Collateral,
        /// New crank fee fund balance
        pub new_balance: Collateral,
    }

    impl PerpEvent for CrankFeeEvent {}
    impl From<CrankFeeEvent> for Event {
        fn from(
            CrankFeeEvent {
                trade_id,
                amount,
                amount_usd,
                old_balance,
                new_balance,
            }: CrankFeeEvent,
        ) -> Self {
            let (trade_id_key, trade_id_val) = match trade_id {
                TradeId::Position(pos_id) => ("pos-id", pos_id.to_string()),
                TradeId::LimitOrder(order_id) => ("order-id", order_id.to_string()),
            };

            Event::new("crank-fee")
                .add_attribute(trade_id_key, trade_id_val)
                .add_attribute("amount", amount.to_string())
                .add_attribute("amount-usd", amount_usd.to_string())
                .add_attribute("old-balance", old_balance.to_string())
                .add_attribute("new-balance", new_balance.to_string())
        }
    }
    impl TryFrom<Event> for CrankFeeEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            let trade_id = match evt.try_u64_attr("pos-id")? {
                Some(pos_id) => TradeId::Position(PositionId::new(pos_id)),
                None => {
                    let order_id = evt.u64_attr("order-id")?;
                    TradeId::LimitOrder(OrderId::new(order_id))
                }
            };

            Ok(CrankFeeEvent {
                trade_id,
                amount: evt.decimal_attr("amount")?,
                amount_usd: evt.decimal_attr("amount-usd")?,
                old_balance: evt.decimal_attr("old-balance")?,
                new_balance: evt.decimal_attr("new-balance")?,
            })
        }
    }

    /// Crank reward was earned by a cranker
    pub struct CrankFeeEarnedEvent {
        /// Which wallet received the fee
        pub recipient: Addr,
        /// Amount allocated to the wallet, in collateral
        pub amount: NonZero<Collateral>,
        /// Amount allocated to the wallet, in USD
        pub amount_usd: NonZero<Usd>,
    }

    impl PerpEvent for CrankFeeEarnedEvent {}
    impl From<CrankFeeEarnedEvent> for Event {
        fn from(
            CrankFeeEarnedEvent {
                recipient,
                amount,
                amount_usd,
            }: CrankFeeEarnedEvent,
        ) -> Self {
            Event::new("crank-fee-claimed")
                .add_attribute("recipient", recipient.to_string())
                .add_attribute("amount", amount.to_string())
                .add_attribute("amount-usd", amount_usd.to_string())
        }
    }
    impl TryFrom<Event> for CrankFeeEarnedEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(CrankFeeEarnedEvent {
                recipient: evt.unchecked_addr_attr("recipient")?,
                amount: evt.non_zero_attr("amount")?,
                amount_usd: evt.non_zero_attr("amount-usd")?,
            })
        }
    }

    /// Emitted when there is insufficient liquidation margin for a fee
    pub struct InsufficientMarginEvent {
        /// Position that had insufficient margin
        pub pos: PositionId,
        /// Type of fee that couldn't be covered
        pub fee_type: FeeType,
        /// Funds available
        pub available: Signed<Collateral>,
        /// Fee amount requested
        pub requested: Signed<Collateral>,
        /// Description of what happened
        pub desc: Option<String>,
    }
    impl From<InsufficientMarginEvent> for Event {
        fn from(
            InsufficientMarginEvent {
                pos,
                fee_type,
                available,
                requested,
                desc,
            }: InsufficientMarginEvent,
        ) -> Self {
            let evt = Event::new(event_key::INSUFFICIENT_MARGIN)
                .add_attribute(event_key::POS_ID, pos.to_string())
                .add_attribute(event_key::FEE_TYPE, fee_type.as_str())
                .add_attribute(event_key::AVAILABLE, available.to_string())
                .add_attribute(event_key::REQUESTED, requested.to_string());
            match desc {
                Some(desc) => evt.add_attribute(event_key::DESC, desc),
                None => evt,
            }
        }
    }
    impl PerpEvent for InsufficientMarginEvent {}

    /// Fee type which can have insufficient margin available
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum FeeType {
        /// There is insufficient active collateral for the liquidation margin.
        Overall,
        /// Insufficient borrow fee portion of the liquidation margin.
        Borrow,
        /// Insufficient delta neutrality fee portion of the liquidation margin.
        DeltaNeutrality,
        /// Insufficient funding payment portion of the liquidation margin.
        Funding,
        /// Insufficient crank fee portion of the liquidation margin.
        Crank,
        /// Protocol-wide insufficient funding payments.
        ///
        /// This means that the protocol itself would reach insolvency if we
        /// paid the funding payments this payment expects.
        FundingTotal,
    }

    impl FeeType {
        /// Represent as a string
        pub fn as_str(self) -> &'static str {
            match self {
                FeeType::Overall => "overall",
                FeeType::Borrow => "borrow",
                FeeType::DeltaNeutrality => "delta-neutrality",
                FeeType::Funding => "funding",
                FeeType::FundingTotal => "funding-total",
                FeeType::Crank => "crank",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::contracts::market::spot_price::SpotPriceConfig;

    use super::*;

    #[test]
    fn trade_fee_open() {
        let config = Config {
            trading_fee_notional_size: "0.01".parse().unwrap(),
            trading_fee_counter_collateral: "0.02".parse().unwrap(),
            ..Config::new(SpotPriceConfig::Manual {
                admin: Addr::unchecked("foo"),
            })
        };
        assert_eq!(
            config
                .calculate_trade_fee_open("-500".parse().unwrap(), "200".parse().unwrap())
                .unwrap(),
            "9".parse().unwrap()
        )
    }

    #[test]
    fn trade_fee_update() {
        let config = Config {
            trading_fee_notional_size: "0.01".parse().unwrap(),
            trading_fee_counter_collateral: "0.02".parse().unwrap(),
            ..Config::new(SpotPriceConfig::Manual {
                admin: Addr::unchecked("foo"),
            })
        };
        assert_eq!(
            config
                .calculate_trade_fee(
                    "-100".parse().unwrap(),
                    "-500".parse().unwrap(),
                    "100".parse().unwrap(),
                    "200".parse().unwrap()
                )
                .unwrap(),
            "6".parse().unwrap()
        );
        assert_eq!(
            config
                .calculate_trade_fee(
                    "-100".parse().unwrap(),
                    "-500".parse().unwrap(),
                    "300".parse().unwrap(),
                    "200".parse().unwrap()
                )
                .unwrap(),
            "4".parse().unwrap()
        );
        assert_eq!(
            config
                .calculate_trade_fee(
                    "-600".parse().unwrap(),
                    "-500".parse().unwrap(),
                    "300".parse().unwrap(),
                    "200".parse().unwrap()
                )
                .unwrap(),
            "0".parse().unwrap()
        );
        assert_eq!(
            config
                .calculate_trade_fee(
                    "-600".parse().unwrap(),
                    "-500".parse().unwrap(),
                    "100".parse().unwrap(),
                    "200".parse().unwrap()
                )
                .unwrap(),
            "2".parse().unwrap()
        );
    }
}
