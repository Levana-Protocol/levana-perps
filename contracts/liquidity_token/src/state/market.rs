use crate::state::*;
use cosmwasm_std::QueryResponse;
use msg::contracts::{
    cw20::{Cw20ReceiveMsg, ReceiverExecuteMsg},
    factory::entry::{MarketInfoResponse, QueryMsg as FactoryQueryMsg},
    liquidity_token::entry::{
        ExecuteMsg as LiquidityTokenExecuteMsg, QueryMsg as LiquidityTokenQueryMsg,
    },
    market::entry::{ExecuteMsg as MarketExecuteMsg, QueryMsg as MarketQueryMsg},
};
use msg::prelude::*;

const MARKET_ID: Item<MarketId> = Item::new(namespace::MARKET_ID);

pub(crate) fn market_init(store: &mut dyn Storage, market_id: MarketId) -> Result<()> {
    MARKET_ID.save(store, &market_id)?;
    Ok(())
}

fn get_market_id(store: &dyn Storage) -> Result<MarketId> {
    MARKET_ID.load(store).map_err(|err| err.into())
}

impl State<'_> {
    pub(crate) fn market_addr(&self, store: &dyn Storage) -> Result<Addr> {
        let market_id = get_market_id(store)?;

        let resp: MarketInfoResponse = self.querier.query_wasm_smart(
            &self.factory_address,
            &FactoryQueryMsg::MarketInfo { market_id },
        )?;
        Ok(resp.market_addr)
    }

    pub(crate) fn market_query_liquidity_token(
        &self,
        store: &dyn Storage,
        msg: LiquidityTokenQueryMsg,
    ) -> Result<QueryResponse> {
        smart_query_no_parse(
            &self.querier,
            self.market_addr(store)?,
            &MarketQueryMsg::LiquidityTokenProxy {
                kind: get_kind(store)?,
                msg,
            },
        )
    }

    pub(crate) fn market_execute_liquidity_token(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        msg: LiquidityTokenExecuteMsg,
    ) -> Result<()> {
        let (msg, send) = match msg {
            // Send needs special handling to ensure the messages to the destination contract come from the proxy, not the market
            LiquidityTokenExecuteMsg::Send {
                contract,
                amount,
                msg,
            } => {
                // This receive message will appear on the destination contract directly from here (the proxy cw20)
                // as far as that destination contract is concerned, that's the cw20 contract itself giving it funds
                let send = ReceiverExecuteMsg::Receive(Cw20ReceiveMsg {
                    sender: sender.clone().into(),
                    amount,
                    msg,
                });
                // However, the actual balances are kept on the market contract, and so we also need to transfer the balance
                // from the market to the destination contract
                let msg = LiquidityTokenExecuteMsg::Transfer {
                    recipient: contract.clone(),
                    amount,
                };
                (msg, Some((contract, send)))
            }
            // SendFrom also needs special handling to ensure the messages to the destination contract come from the proxy, not the market
            LiquidityTokenExecuteMsg::SendFrom {
                owner,
                contract,
                amount,
                msg,
            } => {
                // same idea as above, the target contract gets receive message saying "cw20 has sent you funds"
                let send = ReceiverExecuteMsg::Receive(Cw20ReceiveMsg {
                    sender: sender.clone().into(),
                    amount,
                    msg,
                });

                // but the balance must be updated on the market contract. In this case going through "transfer from"
                // which will ensure the correct approval gating etc.
                let msg = LiquidityTokenExecuteMsg::TransferFrom {
                    owner,
                    recipient: contract.clone(),
                    amount,
                };
                (msg, Some((contract, send)))
            }
            _ => (msg, None),
        };
        ctx.response.add_execute_submessage_oneshot(
            self.market_addr(ctx.storage)?,
            &MarketExecuteMsg::LiquidityTokenProxy {
                sender: sender.into(),
                kind: get_kind(ctx.storage)?,
                msg,
            },
        )?;

        // We need to sequence this submessage after the previous one to ensure the
        // receiving contract has the expected balance.
        if let Some((contract, send)) = send {
            ctx.response
                .add_execute_submessage_oneshot(contract, &send)?;
        }

        Ok(())
    }
}
