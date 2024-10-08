use perpswap::prelude::*;

use super::{State, StateContext};

pub(super) const TOKEN_INFO: Map<&Addr, TokenInfo> = Map::new(namespace::FAUCET_TOKEN_INFO);
const CW20_CODE_ID: Item<u64> = Item::new(namespace::CW20_CODE_ID);
pub(super) const FAUCET_TOKENS: Map<&str, Addr> = Map::new(namespace::FAUCET_TOKENS);
pub(super) const FAUCET_TOKENS_TRADE: Map<(&str, u32), Addr> =
    Map::new(namespace::FAUCET_TOKENS_TRADE);
pub(super) const NEXT_TOKEN: Item<TokenInfo> = Item::new(namespace::FAUCET_NEXT_TOKEN);

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct TokenInfo {
    pub(crate) name: String,
    pub(crate) trading_competition_index: Option<u32>,
    pub(crate) tap_amount: Number,
}

pub(crate) fn set_cw20_code_id(store: &mut dyn Storage, cw20_code_id: u64) -> Result<()> {
    CW20_CODE_ID.save(store, &cw20_code_id)?;
    Ok(())
}

pub(crate) fn get_cw20_code_id(store: &mut dyn Storage) -> Result<u64> {
    CW20_CODE_ID.load(store).map_err(|e| e.into())
}

pub(crate) fn get_token(
    store: &dyn Storage,
    token_name: &str,
    trading_competition_index: Option<u32>,
) -> Result<Option<Addr>> {
    Ok(match trading_competition_index {
        Some(index) => FAUCET_TOKENS_TRADE.may_load(store, (token_name, index))?,
        None => FAUCET_TOKENS.may_load(store, token_name)?,
    })
}

pub(crate) fn get_next_index(store: &dyn Storage, token_name: &str) -> Result<u32> {
    Ok(
        match FAUCET_TOKENS_TRADE
            .prefix(token_name)
            .keys(store, None, None, cosmwasm_std::Order::Descending)
            .next()
            .transpose()?
        {
            Some(x) => x + 1,
            None => 1,
        },
    )
}

pub(crate) fn set_next_token(store: &mut dyn Storage, token_info: &TokenInfo) -> Result<()> {
    NEXT_TOKEN.save(store, token_info).map_err(|e| e.into())
}

impl State<'_> {
    pub(crate) fn save_next_token(
        &self,
        ctx: &mut StateContext,
        new_contract: &Addr,
    ) -> Result<()> {
        let token_info = NEXT_TOKEN.load(ctx.storage)?;
        NEXT_TOKEN.remove(ctx.storage);
        TOKEN_INFO.save(ctx.storage, new_contract, &token_info)?;
        match token_info.trading_competition_index {
            None => FAUCET_TOKENS.save(ctx.storage, &token_info.name, new_contract)?,
            Some(index) => {
                FAUCET_TOKENS_TRADE.save(ctx.storage, (&token_info.name, index), new_contract)?
            }
        }
        self.set_tap_amount(
            ctx,
            msg::contracts::faucet::entry::FaucetAsset::Cw20(new_contract.into()),
            token_info.tap_amount,
        )?;
        Ok(())
    }
}
