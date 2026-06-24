use anyhow::{Context, Result};
use perps_exes::{
    config::MainnetFactories, contracts::Factory, prelude::MarketContract, PerpsNetwork,
};
use perpswap::{number::UnsignedDecimal, storage::MarketId};

use crate::cli::Opt;

#[derive(clap::Parser)]
pub(super) struct CloseoutReportOpt {
    /// Factory identifier from config, or a raw factory contract address
    #[clap(long)]
    factory: String,
    /// Required when --factory is a raw contract address
    #[clap(long)]
    network: Option<PerpsNetwork>,
    /// Include markets that have not been put into close-all-positions mode
    #[clap(long)]
    include_open_markets: bool,
}

impl CloseoutReportOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        let factory = load_factory(&opt, &self.factory, self.network).await?;

        tracing::info!("Inspecting factory {}", factory);

        for market in factory.get_markets().await? {
            let market_id: MarketId = market.market_id;
            let contract = MarketContract::new(market.market);
            let close_status = contract.close_status().await?;

            if !self.include_open_markets && !close_status.close_all_positions {
                continue;
            }

            let has_residuals = close_status.open_positions > 0
                || close_status.limit_orders > 0
                || close_status.liquidity_providers > 0
                || close_status.deferred_execution_items > 0
                || !close_status.liquidity.locked.is_zero()
                || !close_status.liquidity.unlocked.is_zero()
                || !close_status.liquidity.total_lp.is_zero()
                || !close_status.liquidity.total_xlp.is_zero()
                || !close_status.fees.wallets.is_zero()
                || !close_status.fees.protocol.is_zero()
                || !close_status.fees.crank.is_zero()
                || !close_status.fees.referral.is_zero()
                || !close_status.delta_neutrality_fee_fund.is_zero()
                || !close_status.actual_collateral.is_zero();

            let state = match (close_status.close_all_positions, has_residuals) {
                (false, _) => "open",
                (true, true) => "closeout-pending",
                (true, false) => "closed-clean",
            };

            tracing::info!(
                "{market_id}: state={state} actual-collateral={} open-positions={} limit-orders={} deferred={} lp-records={} liquidity=({} locked, {} unlocked, {} lp, {} xlp) fees=({}, {}, {}, {}) dnf={} next-crank={:?}",
                close_status.actual_collateral,
                close_status.open_positions,
                close_status.limit_orders,
                close_status.deferred_execution_items,
                close_status.liquidity_providers,
                close_status.liquidity.locked,
                close_status.liquidity.unlocked,
                close_status.liquidity.total_lp,
                close_status.liquidity.total_xlp,
                close_status.fees.wallets,
                close_status.fees.protocol,
                close_status.fees.crank,
                close_status.fees.referral,
                close_status.delta_neutrality_fee_fund,
                close_status.next_crank,
            );
        }

        Ok(())
    }
}

async fn load_factory(opt: &Opt, factory: &str, network: Option<PerpsNetwork>) -> Result<Factory> {
    if let Ok(mainnet_factory) = MainnetFactories::load()?.get(factory) {
        let basic = opt.load_basic_app(mainnet_factory.network).await?;
        let factory = Factory::from_contract(basic.cosmos.make_contract(mainnet_factory.address));
        return Ok(factory);
    }

    let network = network.context("When --factory is a raw address, --network is required")?;
    let basic = opt.load_basic_app(network).await?;
    let address = factory
        .parse()
        .with_context(|| format!("Invalid factory address: {factory}"))?;
    let factory = Factory::from_contract(basic.cosmos.make_contract(address));
    Ok(factory)
}
