use crate::state::*;
use anyhow::Result;
use cosmwasm_std::{Addr, BankMsg, Coin, CosmosMsg, Uint128};
use cw_storage_plus::{Item, Map};
use msg::contracts::{
    cw20::entry::{ExecuteMsg as Cw20ExecuteMsg, QueryMsg as Cw20QueryMsg, TokenInfoResponse},
    faucet::{
        entry::{FaucetAsset, TappersResp},
        error::FaucetError,
        events::TapEvent,
    },
};
use msg::prelude::*;
use shared::namespace;

const LAST_TAP_TIMESTAMP: Map<&Addr, Timestamp> = Map::new(namespace::LAST_TAP_TIMESTAMP);
const CW20_TOKEN_INFO: Map<&Addr, TokenInfoResponse> = Map::new(namespace::CW20_TOKEN_INFO);
const TAP_LIMIT: Item<Option<u32>> = Item::new(namespace::TAP_LIMIT);
const CW20_TAP_AMOUNT: Map<&Addr, Number> = Map::new(namespace::CW20_TAP_AMOUNT);
const NATIVE_TAP_AMOUNT: Map<String, Number> = Map::new(namespace::NATIVE_TAP_AMOUNT);

const DEFAULT_CW20_TAP_AMOUNT: &str = "1000";
const DEFAULT_NATIVE_TAP_AMOUNT: &str = "10";
const NATIVE_DECIMAL_PLACES: u32 = 6; // differs per denom?

impl State<'_> {
    pub(crate) fn tap_limit(&self, store: &dyn Storage) -> Result<Option<u32>> {
        TAP_LIMIT.load(store).map_err(|err| err.into())
    }

    pub(crate) fn last_tap_timestamp(
        &self,
        store: &dyn Storage,
        addr: &Addr,
    ) -> Result<Option<Timestamp>> {
        LAST_TAP_TIMESTAMP
            .may_load(store, addr)
            .map_err(|err| err.into())
    }

    /// Like [Self::validate_tap] but uses a [FaucetError]
    pub(crate) fn validate_tap_faucet_error(
        &self,
        store: &dyn Storage,
        recipient: &Addr,
        assets: &[FaucetAsset],
    ) -> Result<Result<(), FaucetError>> {
        let now = self.now();

        for asset in assets {
            match self.already_tapped_trading_competition(store, recipient, asset)? {
                AlreadyTappedTradingCompetition::NotTradingCompetition => (),
                AlreadyTappedTradingCompetition::AlreadyTapped(cw20) => {
                    return Ok(Err(FaucetError::AlreadyTapped { cw20 }))
                }
                AlreadyTappedTradingCompetition::DidNotTap(_) => (),
            }
        }

        if let Some(tap_limit) = self.tap_limit(store)? {
            if let Some(last_tap) = self.last_tap_timestamp(store, recipient)? {
                let elapsed = now.checked_sub(last_tap, "validate_tap_faucet_error")?;
                let tap_limit = Duration::from_seconds(u64::from(tap_limit));

                if elapsed < tap_limit {
                    let time_remaining = tap_limit - elapsed;

                    return Ok(Err(FaucetError::TooSoon {
                        wait_secs: time_remaining.as_ms_decimal_lossy()
                            / Decimal256::from_str("1000").unwrap(),
                    }));
                }
            }
        }

        Ok(Ok(()))
    }

    pub(crate) fn validate_tap(
        &self,
        store: &dyn Storage,
        recipient: &Addr,
        assets: &[FaucetAsset],
    ) -> Result<()> {
        // See docs on validate_tap_faucet_error. Outer Err means there was some
        // problem with storage, inner error indicates whether the wallet can
        // tap or not.
        self.validate_tap_faucet_error(store, recipient, assets)?
            .map_err(|e| match e {
                FaucetError::TooSoon { wait_secs } => perp_anyhow_data!(
                    ErrorId::Exceeded,
                    ErrorDomain::Faucet,
                    e,
                    "You can tap the faucet again in {}",
                    PrettyTimeRemaining(wait_secs),
                ),
                FaucetError::AlreadyTapped { cw20: _ } => perp_anyhow!(
                    ErrorId::Exceeded,
                    ErrorDomain::Faucet,
                    "You cannot tap a trading competition faucet more than once"
                ),
            })
    }

    // only available in mutable for now, simplifies caching mechanism
    pub(crate) fn tap_amount(&self, ctx: &mut StateContext, asset: &FaucetAsset) -> Result<Number> {
        match asset {
            FaucetAsset::Cw20(addr) => {
                let addr = addr.validate(self.api)?;
                match CW20_TAP_AMOUNT.may_load(ctx.storage, &addr)? {
                    Some(amount) => Ok(amount),
                    None => {
                        let amount = Number::try_from(DEFAULT_CW20_TAP_AMOUNT)?;
                        CW20_TAP_AMOUNT.save(ctx.storage, &addr, &amount)?;
                        Ok(amount)
                    }
                }
            }
            FaucetAsset::Native(denom) => {
                match NATIVE_TAP_AMOUNT.may_load(ctx.storage, denom.clone())? {
                    Some(amount) => Ok(amount),
                    None => {
                        let amount = Number::try_from(DEFAULT_NATIVE_TAP_AMOUNT)?;
                        NATIVE_TAP_AMOUNT.save(ctx.storage, denom.clone(), &amount)?;
                        Ok(amount)
                    }
                }
            }
        }
    }
    // only available in mutable for now, simplifies caching mechanism
    pub(crate) fn cw20_token_info(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
    ) -> Result<TokenInfoResponse> {
        match CW20_TOKEN_INFO.may_load(ctx.storage, addr)? {
            Some(info) => Ok(info),
            None => {
                let token_info = self
                    .querier
                    .query_wasm_smart(addr, &Cw20QueryMsg::TokenInfo {})?;

                CW20_TOKEN_INFO.save(ctx.storage, addr, &token_info)?;

                Ok(token_info)
            }
        }
    }

    pub(crate) fn save_last_tap(&self, ctx: &mut StateContext, recipient: &Addr) -> Result<()> {
        let now = self.now();
        // always save, since config may change in-between executions
        LAST_TAP_TIMESTAMP.save(ctx.storage, recipient, &now)?;
        Ok(())
    }

    pub(crate) fn tap(
        &self,
        ctx: &mut StateContext,
        asset: FaucetAsset,
        recipient: &Addr,
        amount: Option<Number>,
    ) -> Result<()> {
        let amount = match amount {
            Some(amount) => amount,
            None => self.tap_amount(ctx, &asset)?,
        };

        if amount < Number::ZERO {
            perp_bail!(
                ErrorId::InvalidAmount,
                ErrorDomain::Faucet,
                "amount must be greater than zero!"
            );
        }

        match &asset {
            FaucetAsset::Cw20(cw20_addr) => {
                let cw20_addr = cw20_addr.validate(self.api)?;
                let token_info = self.cw20_token_info(ctx, &cw20_addr)?;
                let cw20_amount = amount
                    .to_u128_with_precision(token_info.decimals.into())
                    .ok_or_else(|| {
                        perp_anyhow!(
                            ErrorId::Conversion,
                            ErrorDomain::Faucet,
                            "unable to convert {} to u128!",
                            amount
                        )
                    })?;

                ctx.response.add_execute_submessage_oneshot(
                    cw20_addr,
                    &Cw20ExecuteMsg::Mint {
                        recipient: recipient.clone().into(),
                        amount: cw20_amount.into(),
                    },
                )?;
            }
            FaucetAsset::Native(denom) => {
                let native_amount = amount
                    .to_u128_with_precision(NATIVE_DECIMAL_PLACES)
                    .ok_or_else(|| {
                        perp_anyhow!(
                            ErrorId::Conversion,
                            ErrorDomain::Faucet,
                            "unable to convert {} to u128!",
                            amount
                        )
                    })?;
                let coin = Coin {
                    denom: denom.clone(),
                    amount: native_amount.into(),
                };

                ctx.response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: recipient.to_string(),
                    amount: vec![coin],
                }));
            }
        }

        let amount_unsigned = amount
            .try_into_non_zero()
            .context("Tried to tap a negative value")?
            .raw();
        self.add_history(ctx, &asset, amount_unsigned)?;

        ctx.response.add_event(TapEvent {
            asset,
            recipient: recipient.clone(),
            amount,
        });

        Ok(())
    }

    pub(crate) fn set_tap_limit(
        &self,
        ctx: &mut StateContext,
        tap_limit: Option<u32>,
    ) -> Result<()> {
        TAP_LIMIT
            .save(ctx.storage, &tap_limit)
            .map_err(|err| err.into())
    }

    pub(crate) fn set_tap_amount(
        &self,
        ctx: &mut StateContext,
        asset: FaucetAsset,
        amount: Number,
    ) -> Result<()> {
        match asset {
            FaucetAsset::Cw20(addr) => {
                let addr = addr.validate(self.api)?;
                CW20_TAP_AMOUNT
                    .save(ctx.storage, &addr, &amount)
                    .map_err(|err| err.into())
            }
            FaucetAsset::Native(denom) => NATIVE_TAP_AMOUNT
                .save(ctx.storage, denom, &amount)
                .map_err(|err| err.into()),
        }
    }

    pub(crate) fn tappers(
        &self,
        store: &dyn Storage,
        start_after: Option<Addr>,
        limit: u32,
    ) -> Result<TappersResp> {
        let start_after = start_after.as_ref().map(Bound::exclusive);
        Ok(TappersResp {
            tappers: LAST_TAP_TIMESTAMP
                .keys(store, start_after, None, Order::Ascending)
                .take(limit.try_into()?)
                .collect::<Result<_, _>>()?,
        })
    }
}

pub(crate) struct PrettyTimeRemaining(pub(crate) Decimal256);

impl Display for PrettyTimeRemaining {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let secs = match Uint128::try_from(self.0.to_uint_floor()) {
            Err(_) => return f.write_str("a really long time"),
            Ok(secs) => secs.u128(),
        }
        .max(1);
        let minutes = if secs < 60 {
            return write!(f, "{}s", secs);
        } else {
            secs / 60
        };
        debug_assert!(minutes > 0);
        let hours = minutes / 60;
        let minutes = minutes % 60;
        let days = hours / 24;
        let hours = hours % 24;

        let mut need_space = false;
        for (number, letter) in [(days, 'd'), (hours, 'h'), (minutes, 'm')] {
            if number > 0 {
                if need_space {
                    write!(f, " {number}{letter}")?;
                } else {
                    need_space = true;
                    write!(f, "{number}{letter}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    fn go(seconds: u64) -> String {
        super::PrettyTimeRemaining(seconds.to_string().parse().unwrap()).to_string()
    }
    #[test]
    fn display_pretty_time() {
        assert_eq!(go(5), "5s");
        assert_eq!(go(300), "5m");
        assert_eq!(go(3600), "1h");
        assert_eq!(go(3660), "1h 1m");
        // Spec implies we shouldn't show seconds in general
        assert_eq!(go(3661), "1h 1m");
        assert_eq!(go(90061), "1d 1h 1m");
    }
}
