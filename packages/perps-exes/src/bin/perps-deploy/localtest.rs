use cosmos::{ContractAdmin, HasAddress, HasAddressHrp, SeedPhrase};
use perps_exes::{PerpApp, PerpsNetwork};
use perpswap::contracts::{
    cw20::{entry::InstantiateMinter, Cw20Coin},
    market::{config::ConfigUpdate, entry::MigrateMsg, spot_price::SpotPriceConfigInit},
};
use perpswap::prelude::*;

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use crate::chain_tests::{
    test_funding_market, test_pnl_on_liquidation, test_set_and_fetch_price, test_update_leverage,
    test_update_max_gains,
};
use crate::{
    chain_tests::{test_update_collateral, test_wallet_balance_decrease},
    cli::Opt,
    instantiate::{
        CollateralSource, Cw20Source, InstantiateMarket, InstantiateParams, InstantiateResponse,
        MarketResponse, ProtocolCodeIds, INITIAL_BALANCE_AMOUNT,
    },
    local_deploy::{self, LocalDeployOpt},
    store_code::{CW20, MARKET},
};

#[derive(clap::Parser)]
pub(crate) struct TestsOpt {
    /// Network to use. Either this or family must be provided.
    #[clap(long, env = "COSMOS_NETWORK")]
    pub(crate) network: PerpsNetwork,
    /// Skip initialization
    #[clap(long)]
    skip_init: bool,
    /// Market we want to interact with
    #[clap(
        long,
        env = "LEVANA_PERP_MARKET_ID",
        global = true,
        default_value = "ATOM_USD"
    )]
    pub market_id: MarketId,
    /// Run only the osmomainnet1 force-withdraw migration test.
    #[clap(long)]
    force_withdraw_migration_test: bool,
    /// LCD endpoint used to query and download osmomainnet1 code.
    #[clap(
        long,
        default_value = "https://rest.cosmos.directory/osmosis",
        env = "OSMOMAINNET_LCD"
    )]
    mainnet_lcd: String,
    /// Directory used to cache downloaded osmomainnet1 wasm blobs.
    #[clap(long, default_value = "target/osmomainnet1-wasm")]
    mainnet_wasm_cache_dir: PathBuf,
}

struct OsmoLocalProcess(Child);

fn kill_osmo_local() {
    match Command::new("docker")
        .arg("stop")
        .arg("osmolocaltest")
        .status()
    {
        Ok(ec) => {
            if ec.success() {
                tracing::info!("Successfully killed osmolocal");
            } else {
                tracing::info!("Killing osmolocal exited with {ec:?}");
            }
        }
        Err(e) => tracing::info!("Problem killing osmolocal: {e:?}"),
    }
}

impl Drop for OsmoLocalProcess {
    fn drop(&mut self) {
        kill_osmo_local()
    }
}

impl OsmoLocalProcess {
    fn launch() -> Result<Self> {
        kill_osmo_local();
        tracing::info!("Going to spawn new osmolocal");
        Ok(OsmoLocalProcess(
            Command::new("./.ci/osmolocal.sh")
                .arg("--no-terminal")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        ))
    }
}

fn init_process(skip_init: bool) -> Result<Option<OsmoLocalProcess>> {
    if skip_init {
        Ok(None)
    } else {
        Ok(Some(OsmoLocalProcess::launch()?))
    }
}

pub(crate) async fn go(opt: Opt, opts: TestsOpt) -> Result<()> {
    let mut ol = init_process(opts.skip_init)?;

    let raw_wallet = opt.wallet.clone().context("No wallet provided")?;
    let network = opts.network;

    if let Some(ol) = &mut ol {
        tracing::info!("Waiting till Network is up");
        wait_till_network_is_up(raw_wallet.clone(), network, ol).await?;
    }

    if opts.force_withdraw_migration_test {
        return force_withdraw_migration_test(opt, opts).await;
    }

    tracing::info!("Going to Deploy");

    let InstantiateResponse {
        factory,
        markets: _,
    } = local_deploy::go(
        opt.clone(),
        LocalDeployOpt {
            network,
            initial_price: "9.5".parse()?,
            collateral_price: "10".parse()?,
        },
    )
    .await?;

    let perp_app = PerpApp::new(raw_wallet, factory, None, opts.market_id, network).await?;

    test_funding_market(&perp_app).await?;
    test_wallet_balance_decrease(&perp_app).await?;
    test_update_collateral(&perp_app).await?;
    test_set_and_fetch_price(&perp_app).await?;
    test_update_leverage(&perp_app).await?;
    test_update_max_gains(&perp_app).await?;
    test_pnl_on_liquidation(&perp_app).await?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct SmartQueryResp<T> {
    data: T,
}

#[derive(serde::Deserialize)]
struct MainnetCodeIds {
    factory: String,
    market: String,
    position_token: String,
    liquidity_token: String,
}

#[derive(serde::Deserialize)]
struct FactoryConfiguredCodeIds {
    market: String,
    position_token: String,
    liquidity_token: String,
}

#[derive(serde::Deserialize)]
struct ContractInfoResp {
    contract_info: ContractInfo,
}

#[derive(serde::Deserialize)]
struct ContractInfo {
    code_id: String,
}

#[derive(serde::Deserialize)]
struct WasmCodeResp {
    data: cosmwasm_std::Binary,
}

async fn force_withdraw_migration_test(opt: Opt, opts: TestsOpt) -> Result<()> {
    let basic = opt.load_basic_app(opts.network).await?;
    let wallet = basic.get_wallet()?;
    let config_testnet =
        perps_exes::config::ConfigTestnet::load_from_opt(opt.config_testnet.as_deref())?;

    tracing::info!("Querying osmomainnet1 code IDs");
    let code_ids = query_osmomainnet1_code_ids(&opts.mainnet_lcd).await?;
    tracing::info!(
        "osmomainnet1 code IDs: factory={}, market={}, position_token={}, liquidity_token={}",
        code_ids.factory,
        code_ids.market,
        code_ids.position_token,
        code_ids.liquidity_token,
    );

    let old_factory = download_mainnet_wasm(
        &opts.mainnet_lcd,
        &opts.mainnet_wasm_cache_dir,
        &code_ids.factory,
    )
    .await?;
    let old_market = download_mainnet_wasm(
        &opts.mainnet_lcd,
        &opts.mainnet_wasm_cache_dir,
        &code_ids.market,
    )
    .await?;
    let old_position = download_mainnet_wasm(
        &opts.mainnet_lcd,
        &opts.mainnet_wasm_cache_dir,
        &code_ids.position_token,
    )
    .await?;
    let old_liquidity = download_mainnet_wasm(
        &opts.mainnet_lcd,
        &opts.mainnet_wasm_cache_dir,
        &code_ids.liquidity_token,
    )
    .await?;

    tracing::info!("Uploading osmomainnet1-aligned wasm blobs to osmolocal");
    let old_factory_code_id = basic.cosmos.store_code_path(wallet, old_factory).await?;
    let old_market_code_id = basic.cosmos.store_code_path(wallet, old_market).await?;
    let old_position_code_id = basic.cosmos.store_code_path(wallet, old_position).await?;
    let old_liquidity_code_id = basic.cosmos.store_code_path(wallet, old_liquidity).await?;
    let cw20_code_id = basic
        .cosmos
        .store_code_path(wallet, opt.get_contract_path(CW20))
        .await?;

    let cw20 = cw20_code_id
        .instantiate(
            wallet,
            "Force withdraw migration collateral",
            vec![],
            perpswap::contracts::cw20::entry::InstantiateMsg {
                name: opts.market_id.get_collateral().to_owned(),
                symbol: opts.market_id.get_collateral().to_owned(),
                decimals: 6,
                initial_balances: vec![Cw20Coin {
                    address: wallet.get_address_string(),
                    amount: INITIAL_BALANCE_AMOUNT.into(),
                }],
                minter: InstantiateMinter {
                    minter: wallet.get_address_string().into(),
                    cap: None,
                },
                marketing: None,
            },
            ContractAdmin::Sender,
        )
        .await?;

    tracing::info!("Instantiating an old-code local market");
    let res = crate::instantiate::instantiate(InstantiateParams {
        opt: &opt,
        basic: &basic,
        config_testnet: &config_testnet,
        code_id_source: crate::instantiate::CodeIdSource::Existing(ProtocolCodeIds {
            factory_code_id: old_factory_code_id,
            position_token_code_id: old_position_code_id,
            liquidity_token_code_id: old_liquidity_code_id,
            market_code_id: old_market_code_id,
        }),
        family: "force-withdraw-migration-local".to_owned(),
        markets: vec![InstantiateMarket {
            market_id: opts.market_id.clone(),
            collateral: CollateralSource::Cw20(Cw20Source::Existing(cw20.get_address())),
            config: ConfigUpdate::default(),
            initial_borrow_fee_rate: "0.01".parse()?,
            spot_price: SpotPriceConfigInit::Manual {
                admin: wallet.get_address_string().into(),
            },
        }],
        trading_competition: false,
        faucet_admin: None,
    })
    .await?;

    let MarketResponse { market_addr, .. } = res
        .markets
        .into_iter()
        .next()
        .context("No market was instantiated")?;
    let market =
        perps_exes::contracts::MarketContract::new(basic.cosmos.make_contract(market_addr));

    market
        .set_price(wallet, "9.5".parse()?, "9.5".parse()?)
        .await
        .context("Setting initial price")?;
    let status = market.status().await?;
    let balance_before = market.get_collateral_balance(&status, wallet).await?;
    let deposit = NonZero::<Collateral>::from_str("100")?;
    market
        .deposit(wallet, &status, deposit)
        .await
        .context("Depositing liquidity into old-code market")?;
    let lp_info_before = market.lp_info(wallet).await?;
    anyhow::ensure!(
        lp_info_before.lp_amount > LpToken::zero(),
        "LP deposit did not create LP shares"
    );

    let order_deposit = NonZero::<Collateral>::from_str("10")?;
    market
        .place_limit_order(
            wallet,
            &status,
            order_deposit,
            "8".parse()?,
            DirectionToBase::Long,
            "2".parse()?,
            "20".parse()?,
        )
        .await
        .context("Placing limit order in old-code market")?;
    market
        .crank(wallet, None)
        .await
        .context("Cranking old-code limit order placement")?;
    let balance_after_setup = market.get_collateral_balance(&status, wallet).await?;

    tracing::info!("Migrating the market to the workspace build");
    let new_market_code_id = basic
        .cosmos
        .store_code_path(wallet, opt.get_contract_path(MARKET))
        .await?;
    basic
        .cosmos
        .make_contract(market_addr)
        .migrate(wallet, new_market_code_id.get_code_id(), MigrateMsg {})
        .await
        .context("Migrating market to workspace build")?;

    let status = market.status().await?;
    let new_order_err = market
        .place_limit_order(
            wallet,
            &status,
            order_deposit,
            "8".parse()?,
            DirectionToBase::Long,
            "2".parse()?,
            "20".parse()?,
        )
        .await
        .expect_err("new-code market unexpectedly accepted a new limit order");
    anyhow::ensure!(
        new_order_err
            .to_string()
            .contains("market is no longer operational"),
        "new limit order failed with unexpected error: {new_order_err:?}"
    );

    tracing::info!("Sweeping discovered limit-order funds through the unpermissioned endpoint");
    basic
        .cosmos
        .make_contract(market_addr)
        .execute(
            wallet,
            vec![],
            perpswap::contracts::market::entry::ExecuteMsg::ForceWithdrawAll { limit: Some(1) },
        )
        .await
        .context("Executing ForceWithdrawAll for limit order")?;

    let balance_after_order = market.get_collateral_balance(&status, wallet).await?;
    anyhow::ensure!(
        balance_after_order > balance_after_setup,
        "Limit order collateral was not returned. before={balance_after_setup}, after={balance_after_order}"
    );

    tracing::info!("Sweeping discovered LP funds through the unpermissioned endpoint");
    basic
        .cosmos
        .make_contract(market_addr)
        .execute(
            wallet,
            vec![],
            perpswap::contracts::market::entry::ExecuteMsg::ForceWithdrawAll { limit: Some(1) },
        )
        .await
        .context("Executing ForceWithdrawAll for LP")?;

    let status = market.status().await?;
    let lp_info_after = market.lp_info(wallet).await?;
    anyhow::ensure!(
        lp_info_after.lp_amount.is_zero()
            && lp_info_after.xlp_amount.is_zero()
            && lp_info_after.available_yield.is_zero(),
        "LP state was not cleared: {lp_info_after:?}"
    );
    let balance_after = market.get_collateral_balance(&status, wallet).await?;
    let balance_shortfall = balance_before
        .checked_sub(balance_after)
        .unwrap_or_else(|_| Collateral::zero());
    let dust_tolerance = Collateral::from_str("0.000001")?;
    anyhow::ensure!(
        balance_shortfall <= dust_tolerance,
        "Collateral was not returned to the LP wallet. before={balance_before}, after={balance_after}"
    );

    tracing::info!(
        "Force-withdraw migration test passed. collateral before={balance_before}, after={balance_after}"
    );

    Ok(())
}

async fn query_osmomainnet1_code_ids(mainnet_lcd: &str) -> Result<MainnetCodeIds> {
    const OSMOMAINNET1_FACTORY: &str =
        "osmo1ssw6x553kzqher0earlkwlxasfm2stnl3ms3ma2zz4tnajxyyaaqlucd45";
    const CODE_IDS_QUERY_BASE64: &str = "eyJjb2RlX2lkcyI6e319";
    let factory_url = format!(
        "{}/cosmwasm/wasm/v1/contract/{}",
        mainnet_lcd.trim_end_matches('/'),
        OSMOMAINNET1_FACTORY
    );
    let factory_resp: ContractInfoResp = reqwest::get(factory_url)
        .await?
        .error_for_status()?
        .json()
        .await?;

    let code_ids_url = format!(
        "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
        mainnet_lcd.trim_end_matches('/'),
        OSMOMAINNET1_FACTORY,
        CODE_IDS_QUERY_BASE64
    );
    let resp: SmartQueryResp<FactoryConfiguredCodeIds> = reqwest::get(code_ids_url)
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(MainnetCodeIds {
        factory: factory_resp.contract_info.code_id,
        market: resp.data.market,
        position_token: resp.data.position_token,
        liquidity_token: resp.data.liquidity_token,
    })
}

async fn download_mainnet_wasm(
    mainnet_lcd: &str,
    cache_dir: &Path,
    code_id: &str,
) -> Result<PathBuf> {
    fs_err::create_dir_all(cache_dir)?;
    let path = cache_dir.join(format!("osmomainnet1-code-{code_id}.wasm"));
    if path.exists() {
        tracing::info!("Using cached mainnet wasm {}", path.display());
        return Ok(path);
    }

    tracing::info!("Downloading osmomainnet1 code ID {code_id}");
    let url = format!(
        "{}/cosmwasm/wasm/v1/code/{}",
        mainnet_lcd.trim_end_matches('/'),
        code_id
    );
    let resp: WasmCodeResp = reqwest::get(url).await?.error_for_status()?.json().await?;
    fs_err::write(&path, resp.data.as_slice())?;
    Ok(path)
}

async fn wait_till_network_is_up(
    wallet: SeedPhrase,
    network: PerpsNetwork,
    ol: &mut OsmoLocalProcess,
) -> Result<()> {
    let total_estimated_seconds = Duration::from_secs(15);
    let retry_seconds = Duration::from_millis(100);
    let total_counter = total_estimated_seconds.as_millis() / retry_seconds.as_millis();

    for counter in 1..=total_counter {
        if counter % 10 == 0 {
            tracing::info!("Trying to connect to the network ({counter}/{total_counter})");
        }

        if let Some(exit_status) = ol.0.try_wait()? {
            anyhow::bail!("localosmo child process exited early with exit status: {exit_status}");
        }

        let builder = network.builder().await?;
        let cosmos = builder.build();
        let cosmos = match cosmos {
            Ok(cosmos) => cosmos,
            Err(_) => {
                tokio::time::sleep(retry_seconds).await;
                continue;
            }
        };
        let address_type = cosmos.get_address_hrp();
        let wallet = wallet.with_hrp(address_type)?;

        let balances = cosmos.all_balances(wallet.get_address()).await;
        if balances.is_ok() {
            return Ok(());
        } else {
            tokio::time::sleep(retry_seconds).await;
        }
    }
    Err(anyhow!("Unable to connect to the network"))
}
