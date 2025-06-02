use crate::cli::Opt;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use cosmos::{ContractAdmin, HasAddress};
use perpswap::{
    contracts::vault::{InstantiateMsg, UsdcAssetInit},
    storage::MarketId,
};
use std::{collections::HashMap, str::FromStr};

#[derive(clap::Parser)]
pub(crate) struct InstantiateVaultOpt {
    /// USDC CW20 token address (mutually exclusive with --usdc-native)
    #[clap(long, group = "usdc")]
    usdc_cw20: Option<String>,

    /// USDC native denom (mutually exclusive with --usdc-cw20)
    #[clap(long, group = "usdc")]
    usdc_native: Option<String>,

    /// Market addresses and allocation bps (format: MarketId:bps,MarketId:bps)
    #[clap(long, value_delimiter = ',')]
    markets: Vec<String>,

    /// Family name for vault deployment
    #[clap(long, env = "PERPS_FAMILY")]
    pub family: String,
}

pub(crate) async fn go(opt: Opt, inst_opt: InstantiateVaultOpt) -> Result<()> {
    let app = opt.load_app(&inst_opt.family).await?;
    let wallet = app.basic.get_wallet()?;
    let tracker = app.tracker;

    let usdc_denom = match (&inst_opt.usdc_cw20, &inst_opt.usdc_native) {
        (Some(addr), None) => {
            app.basic
                .cosmos
                .make_contract(addr.parse().context("Invalid CW20 address")?);

            UsdcAssetInit::CW20 {
                address: addr.clone(),
            }
        }
        (None, Some(denom)) => UsdcAssetInit::Native {
            denom: denom.clone(),
        },
        _ => {
            return Err(anyhow!(
                "Specify either --usdc-cw20 or --usdc-native, not both"
            ))
        }
    };

    tracing::info!("Retrieving vault code id for family: {}", inst_opt.family);
    let code_id = tracker
        .require_code_by_type(&opt, "vault")
        .await
        .context("Failed to retrieve vault code ID from tracker")?;
    tracing::info!("Vault code ID: {}", code_id);

    tracing::info!("Retrieving factory for family: {}", inst_opt.family);
    let factory = tracker
        .get_factory(&inst_opt.family)
        .await
        .context("Failed to retrieve factory contract")?;
    tracing::info!("Factory contract: {:?}", factory);

    let mut markets_allocation_bps = HashMap::new();
    let total_bps: u16 = inst_opt
        .markets
        .iter()
        .map(|market_str| {
            let parts: Vec<&str> = market_str.split(':').collect();
            if parts.len() != 2 {
                return Err(anyhow!(
                    "Invalid market format: {}. Expected MarketId:bps",
                    market_str
                ));
            }
            parts[1]
                .parse::<u16>()
                .context(format!("Invalid bps for market {}: {}", parts[0], parts[1]))
        })
        .collect::<Result<Vec<u16>>>()?
        .iter()
        .sum();
    if total_bps > 10_000 {
        return Err(anyhow!("Total bps ({}) exceeds 10000", total_bps));
    }

    for market_str in inst_opt.markets.iter() {
        let parts: Vec<&str> = market_str.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid market format: {}. Expected MarketId:bps",
                market_str
            ));
        }
        let market_id = parts[0];
        let bps: u16 = parts[1].parse().context(format!(
            "Invalid bps for market {}: {}",
            market_id, parts[1]
        ))?;

        let market_id =
            MarketId::from_str(market_id).context(format!("Invalid market ID: {}", market_id))?;

        tracing::info!("Market ID: {}, BPS: {}", market_id, bps);

        let market_info = factory
            .get_market(market_id)
            .await
            .context("Failed to get market {} from factory: {}")?;

        let market_addr = market_info.market.get_address_string();
        tracing::info!("Resolved market {} address: {}", market_str, market_addr);

        markets_allocation_bps.insert(market_addr, bps);
    }

    let label = format!("vault-{}", Utc::now().timestamp());
    let msg = InstantiateMsg {
        usdc_denom,
        governance: wallet.get_address_string(),
        markets_allocation_bps,
    };

    let vault = code_id
        .instantiate(wallet, label, vec![], msg, ContractAdmin::Sender)
        .await
        .context("Failed to instantiate vault contract")?;

    let vault_addr = vault.get_address();

    tracker
        .instantiate(
            wallet,
            &[(code_id.get_code_id(), vault_addr)],
            "vault".to_string(),
        )
        .await
        .context("Failed to log vault to tracker")?;

    tracing::info!("Vault instantiated at {}", vault_addr);
    Ok(())
}
