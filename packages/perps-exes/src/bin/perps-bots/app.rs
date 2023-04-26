mod crank;
pub(crate) mod factory;
mod liquidity;
mod nibb;
mod price;
pub(crate) mod status_collector;
mod trader;
mod utilization;

use std::sync::Arc;

use anyhow::Result;
use cosmos::Contract;
use cosmos::Cosmos;
use cosmos::CosmosNetwork;
use cosmos::HasAddress;
use cosmos::HasAddressType;
use cosmos::Wallet;
use parking_lot::RwLock;
use perps_exes::config::Config;
use reqwest::Client;
use tokio::sync::Mutex;

use crate::cli::get_deployment_config;
use crate::{cli::Opt, endpoints::epochs::Epochs};

use self::{
    factory::FactoryInfo,
    status_collector::{Status, StatusCategory, StatusCollector},
};

#[derive(Clone)]
pub(crate) struct App {
    status_collector: StatusCollector,
    epochs: Epochs,
    factory: Arc<RwLock<Arc<FactoryInfo>>>,
    pub(crate) frontend_info: Arc<FrontendInfo>,
    pub(crate) faucet_bot: Arc<FaucetBot>,
    pub(crate) cosmos: Cosmos,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FrontendInfo {
    network: CosmosNetwork,
    price_api: &'static str,
    explorer: &'static str,
}

pub(crate) struct FaucetBot {
    pub(crate) wallet: Wallet,
    pub(crate) client: Client,
    pub(crate) hcaptcha_secret: String,
    pub(crate) faucet: Contract,
}

impl App {
    pub(crate) async fn load(opt: Opt) -> Result<Self> {
        let config = Config::load()?;
        let deployment_config = get_deployment_config(config, &opt)?;
        let config = Arc::new(deployment_config);
        let mut builder = config.network.builder();
        if let Some(grpc_url) = &opt.grpc_url {
            builder.grpc_url = grpc_url.clone();
        }
        let cosmos = builder.build().await?;
        let client = Client::builder().user_agent("perps-bots").build()?;
        let status_collector = StatusCollector {
            collections: Default::default(),
            cosmos_network: config.network,
            client: client.clone(),
            cosmos: cosmos.clone(),
        };

        let faucet_bot_wallet = opt.get_faucet_bot_wallet(cosmos.get_address_type())?;
        let gas_wallet = Arc::new(Mutex::new(opt.get_gas_wallet(cosmos.get_address_type())?));
        status_collector.track_gas_funds(
            config.faucet,
            "faucet",
            config.min_gas.faucet,
            gas_wallet.clone(),
        );
        status_collector.track_gas_funds(
            *faucet_bot_wallet.get_address(),
            "faucet-bot",
            config.min_gas.faucet_bot,
            gas_wallet.clone(),
        );
        let faucet_contract = cosmos.make_contract(config.faucet);

        let frontend_info = Arc::new(FrontendInfo {
            network: config.network,
            price_api: config.price_api,
            explorer: config.explorer,
        });

        let factory = status_collector
            .start_get_factory(cosmos.clone(), config.clone())
            .await?;
        match &config.price_wallet {
            Some(price_wallet) => {
                status_collector
                    .start_price(
                        cosmos.clone(),
                        client.clone(),
                        config.clone(),
                        factory.clone(),
                        price_wallet.clone(),
                        gas_wallet.clone(),
                    )
                    .await?
            }
            None => status_collector.add_status(
                StatusCategory::Price,
                "disabled",
                Status::success("Oracle not running", None),
            ),
        }
        let epochs = Epochs::default();
        status_collector
            .start_crank_bot(
                cosmos.clone(),
                config.clone(),
                epochs.clone(),
                factory.clone(),
                gas_wallet.clone(),
            )
            .await?;
        match &config.nibb {
            None => status_collector.add_status(
                StatusCategory::Nibb,
                "disabled",
                Status::success("Balancer bots not running", None),
            ),
            Some(nibb_config) => {
                status_collector
                    .start_perps_nibb(
                        cosmos.clone(),
                        factory.clone(),
                        config.clone(),
                        nibb_config.clone(),
                        Arc::new(config.wallet_manager.get_wallet("NIBB bot")?),
                        gas_wallet.clone(),
                    )
                    .await?;
            }
        }

        if config.liquidity {
            liquidity::Liquidity {
                cosmos: cosmos.clone(),
                factory_info: factory.clone(),
                status_collector: status_collector.clone(),
                wallet: config.wallet_manager.get_wallet("liquidity")?,
                config: config.clone(),
                gas_wallet: gas_wallet.clone(),
            }
            .start();
        }

        if config.utilization {
            utilization::Utilization {
                cosmos: cosmos.clone(),
                factory_info: factory.clone(),
                status_collector: status_collector.clone(),
                wallet: config.wallet_manager.get_wallet("utilization")?,
                config: config.clone(),
                gas_wallet: gas_wallet.clone(),
            }
            .start();
        }

        for index in 1..=config.traders {
            trader::Trader {
                cosmos: cosmos.clone(),
                factory_info: factory.clone(),
                status_collector: status_collector.clone(),
                wallet: config
                    .wallet_manager
                    .get_wallet(format!("Trader #{index}"))?,
                config: config.clone(),
                gas_wallet: gas_wallet.clone(),
                index,
            }
            .start();
        }

        Ok(App {
            status_collector,
            epochs,
            factory,
            frontend_info,
            faucet_bot: Arc::new(FaucetBot {
                wallet: faucet_bot_wallet,
                client,
                hcaptcha_secret: opt.hcaptcha_secret,
                faucet: faucet_contract,
            }),
            cosmos,
        })
    }

    pub(crate) fn get_status_collector(&self) -> &StatusCollector {
        &self.status_collector
    }

    pub(crate) fn get_epochs(&self) -> &Epochs {
        &self.epochs
    }

    pub(crate) fn get_factory_info(&self) -> Arc<FactoryInfo> {
        self.factory.read().clone()
    }
}
