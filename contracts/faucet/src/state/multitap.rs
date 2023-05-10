//! Functionality around the multitap code.
//!
//! Multitap is the second generation of tapping functionality, allowing for
//! batching and more direct control of the faucet amounts based on asset type.

use cosmwasm_std::Coin;
use msg::contracts::faucet::entry::{FaucetAsset, GasAllowance, MultitapRecipient};
use msg::prelude::*;

use super::gas_coin::get_gas_allowance;
use super::{State, StateContext};

const NAMED_AMOUNT: Map<&str, Decimal256> = Map::new("NAMED_AMOUNT");

impl State<'_> {
    pub(crate) fn multitap(
        &self,
        ctx: &mut StateContext,
        recipients: Vec<MultitapRecipient>,
    ) -> Result<()> {
        let gas_allowance = get_gas_allowance(ctx.storage)?;
        recipients
            .into_iter()
            .try_for_each(|recipient| self.multitap_single(ctx, recipient, &gas_allowance))
    }

    fn multitap_single(
        &self,
        ctx: &mut StateContext,
        MultitapRecipient { addr, assets }: MultitapRecipient,
        gas_allowance: &Option<GasAllowance>,
    ) -> Result<()> {
        let addr = addr.validate(self.api)?;

        // First level of Result handles storage issues. For those we want to fail, so
        // we use the question mark. If the outer layer succeeds, then we check the
        // inner result to see if the wallet is eligible for tapping. If not, we simply
        // skip. This allows multitapping from the faucet bot to be resilient in the
        // face of getting invalid addresses in its queue.
        if let Err(e) = self.validate_tap_faucet_error(ctx.storage, &addr)? {
            ctx.response
                .add_event(Event::new(&addr).add_attribute("wait_secs", e.wait_secs.to_string()));
            return Ok(());
        }
        self.save_last_tap(ctx, &addr)?;
        ctx.response
            .add_event(Event::new(&addr).add_attribute("success", "success"));

        // Top off the gas
        if let Some(GasAllowance { denom, amount }) = gas_allowance {
            let Coin {
                denom: curr_denom,
                amount: curr_amount,
            } = self.querier.query_balance(&addr, denom)?;
            debug_assert_eq!(denom, &curr_denom);
            if curr_amount < *amount {
                self.tap(
                    ctx,
                    FaucetAsset::Native(curr_denom),
                    &addr,
                    Some(Decimal256::from_atomics(amount - curr_amount, 6)?.into_signed()),
                )?;
            }
        };

        assets
            .into_iter()
            .try_for_each(|asset| self.multitap_single_asset(ctx, &addr, asset))
    }

    fn multitap_single_asset(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        asset: FaucetAsset,
    ) -> Result<()> {
        match self.get_multitap_amount(ctx.storage, &asset)? {
            Some(amount) => self.tap(ctx, asset, addr, Some(amount.into_signed())),
            None => Ok(()),
        }
    }

    pub(crate) fn set_multitap_amount(
        &self,
        ctx: &mut StateContext,
        name: &str,
        amount: Decimal256,
    ) -> Result<()> {
        NAMED_AMOUNT
            .save(ctx.storage, name, &amount)
            .map_err(|e| e.into())
    }

    pub(crate) fn get_multitap_amount(
        &self,
        store: &dyn Storage,
        asset: &FaucetAsset,
    ) -> Result<Option<Decimal256>> {
        Ok(match asset {
            FaucetAsset::Cw20(addr) => {
                let addr = addr.validate(self.api)?;
                match super::tokens::TOKEN_INFO.may_load(store, &addr)? {
                    None => None,
                    Some(info) => self.get_multitap_amount_by_name(store, &info.name)?,
                }
            }
            // This may change in the future, but no tapping of native coins for the moment
            FaucetAsset::Native(_) => None,
        })
    }

    pub(crate) fn get_multitap_amount_by_name(
        &self,
        store: &dyn Storage,
        name: &str,
    ) -> Result<Option<Decimal256>> {
        Ok(match NAMED_AMOUNT.may_load(store, name)? {
            Some(amount) => Some(amount),
            None => match name {
                "ATOM" => Some("1000".parse().unwrap()),
                "ETH" => Some("1".parse().unwrap()),
                "BTC" => Some("1".parse().unwrap()),
                "USDC" => Some("10000".parse().unwrap()),
                _ => None,
            },
        })
    }
}
