use perpswap::{contracts::factory::entry::CodeIds, prelude::*};

use super::{
    liquidity_token::liquidity_token_code_id, market::get_market_code_id,
    position_token::position_token_code_id,
};

pub(crate) fn get_code_ids(store: &dyn Storage) -> Result<CodeIds> {
    let counter_trade = crate::state::countertrade::COUNTER_TRADE_CODE_ID
        .may_load(store)?
        .map(|item| item.into());
    let vault = crate::state::vault::VAULT_CODE_ID
        .may_load(store)?
        .map(|item| item.into());
    Ok(CodeIds {
        market: get_market_code_id(store)?.into(),
        position_token: position_token_code_id(store)?.into(),
        liquidity_token: liquidity_token_code_id(store)?.into(),
        counter_trade,
        vault,
    })
}
