use cosmwasm_std::{Addr, MessageInfo, Order, Storage};
use cw_storage_plus::Map;
use perpswap::namespace;
use perpswap::prelude::*;
use perpswap::{
    contracts::factory::entry::ShutdownStatus,
    shutdown::{ShutdownEffect, ShutdownImpact, ShutdownWallet},
};

use super::{
    auth::{get_kill_switch, get_wind_down},
    market::MARKET_ADDRS,
    StateContext,
};

const SHUTDOWNS: Map<(&Addr, ShutdownImpact), ()> = Map::new(namespace::SHUTDOWNS);

pub(crate) fn get_shutdown_status(
    store: &dyn Storage,
    market_id: &MarketId,
) -> Result<ShutdownStatus> {
    let addr = MARKET_ADDRS.load(store, market_id)?;
    Ok(ShutdownStatus {
        disabled: SHUTDOWNS
            .prefix(&addr)
            .keys(store, None, None, Order::Ascending)
            .collect::<Result<_, _>>()?,
    })
}

pub(crate) fn shutdown(
    ctx: &mut StateContext,
    info: &MessageInfo,
    markets: Vec<MarketId>,
    impacts: Vec<ShutdownImpact>,
    effect: ShutdownEffect,
) -> Result<()> {
    let kill_switch = get_kill_switch(ctx.storage)?;
    let wind_down = get_wind_down(ctx.storage)?;
    let shutdown_wallet = if kill_switch == info.sender {
        ShutdownWallet::KillSwitch
    } else if wind_down == info.sender {
        ShutdownWallet::WindDown
    } else {
        perp_bail!(
            ErrorId::Auth,
            ErrorDomain::Factory,
            "Shutdown actions can only be called by kill switch ({kill_switch}) and wind down ({wind_down}) wallets, executed by {}",
            info.sender
        );
    };

    // Avoid interleaving reads and writes, proactively do all the lookups
    let market_addrs = if markets.is_empty() {
        MARKET_ADDRS
            .range(ctx.storage, None, None, cosmwasm_std::Order::Ascending)
            .map(|res| res.map(|x| x.1))
            .collect::<Result<Vec<_>, _>>()
    } else {
        markets
            .into_iter()
            .map(|market_id| MARKET_ADDRS.load(ctx.storage, &market_id))
            .collect()
    }?;

    for market_addr in market_addrs {
        if impacts.is_empty() {
            shutdown_market(
                ctx,
                market_addr,
                effect,
                shutdown_wallet,
                enum_iterator::all(),
            )?;
        } else {
            shutdown_market(
                ctx,
                market_addr,
                effect,
                shutdown_wallet,
                impacts.iter().copied(),
            )?;
        }
    }

    Ok(())
}

fn shutdown_market(
    ctx: &mut StateContext,
    market_addr: Addr,
    effect: ShutdownEffect,
    shutdown_wallet: ShutdownWallet,
    impacts: impl Iterator<Item = ShutdownImpact>,
) -> Result<()> {
    for impact in impacts {
        let key = (&market_addr, impact);
        let is_disabled = SHUTDOWNS.has(ctx.storage, key);
        match (effect, is_disabled) {
            // Handle the two no-op cases without checking perms
            (ShutdownEffect::Disable, true) => (),
            (ShutdownEffect::Enable, false) => (),

            (ShutdownEffect::Disable, false) => {
                impact.ensure_can_perform(shutdown_wallet)?;
                SHUTDOWNS.save(ctx.storage, key, &())?;
            }
            (ShutdownEffect::Enable, true) => {
                impact.ensure_can_perform(shutdown_wallet)?;
                SHUTDOWNS.remove(ctx.storage, key);
            }
        }
    }

    Ok(())
}
