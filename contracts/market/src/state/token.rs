use std::collections::hash_map::Entry;

use crate::state::*;

use cw_storage_plus::Item;
use msg::{
    contracts::cw20::entry::{QueryMsg as Cw20QueryMsg, TokenInfoResponse},
    token::{Token, TokenInit},
};
use perpswap::prelude::*;

pub(super) const TOKEN: Item<Token> = Item::new(namespace::TOKEN);

pub(crate) fn token_init(
    store: &mut dyn Storage,
    querier: &QuerierWrapper,
    init: TokenInit,
) -> Result<()> {
    let token = match init {
        TokenInit::Cw20 { addr } => {
            let resp: TokenInfoResponse =
                querier.query_wasm_smart(addr.as_str(), &Cw20QueryMsg::TokenInfo {})?;

            Token::Cw20 {
                addr,
                decimal_places: resp.decimals,
            }
        }

        TokenInit::Native {
            denom,
            decimal_places,
        } => Token::Native {
            denom,
            decimal_places,
        },
    };

    TOKEN.save(store, &token).map_err(|err| err.into())
}

impl State<'_> {
    pub(crate) fn get_token(&self, store: &dyn Storage) -> Result<&Token> {
        self.token_cache
            .get_or_try_init(|| TOKEN.load(store).map_err(|err| err.into()))
    }

    // sends a coin to an addr
    // does nothing if the coin has 0 amount
    pub(crate) fn add_token_transfer_msg(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        let token = self.get_token(ctx.storage)?;
        if cfg!(debug_assertions) {
            let balance = token.query_balance(&self.querier, &self.env.contract.address)?;
            if balance < amount.raw() {
                let msg = format!(
                    "NOOOOO! trying to send {}, but we only have {} in the contract wallet!",
                    amount, balance
                );

                match token {
                    Token::Cw20 { .. } => {
                        perp_bail!(ErrorId::Cw20Funds, ErrorDomain::Wallet, "{}", msg);
                    }
                    Token::Native { .. } => {
                        perp_bail!(ErrorId::NativeFunds, ErrorDomain::Wallet, "{}", msg);
                    }
                }
            }
        }
        let entry = ctx.fund_transfers.entry(addr.clone());
        match entry {
            Entry::Occupied(mut entry) => {
                let new_value = entry.get().checked_add(amount.raw())?;
                entry.insert(new_value);
            }
            Entry::Vacant(entry) => {
                entry.insert(amount);
            }
        }

        Ok(())
    }
}
