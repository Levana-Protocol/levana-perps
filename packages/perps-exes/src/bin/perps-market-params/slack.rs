use std::collections::HashMap;

use anyhow::{anyhow, Context};
use chrono::{DateTime, Utc};
use cosmos::{Address, CosmosNetwork};
use reqwest::{Client, Url};

use crate::{
    coingecko::{CMCExchange, CmcExchangeInfo, CmcMarketPair, Coin},
    market_param::{AssetName, MarketsConfig, NotionalAsset},
};

pub(crate) struct HttpApp {
    webhook: Option<Url>,
    client: Client,
    cmc_key: String,
}

impl HttpApp {
    pub(crate) fn new(webhook: Option<Url>, cmc_key: String) -> Self {
        let client = Client::new();
        HttpApp {
            webhook,
            client,
            cmc_key,
        }
    }

    pub(crate) async fn send_notification(
        &self,
        message: String,
        description: String,
    ) -> anyhow::Result<()> {
        match &self.webhook {
            Some(webhook) => {
                let value = serde_json::json!(
                {
                    "text": "Market Parameter alert",
                    "blocks": [
                        {
                            "type": "header",
                            "text": {
                    "type": "plain_text",
                    "text": message.to_string(),
                            }
                        },
                        {
                    "type": "section",
                    "block_id": "section567",
                    "text": {
                    "type": "mrkdwn",
                    "text": description
                    },
                    "accessory": {
                    "type": "image",
                    "image_url": "https://static.levana.finance/icons/levana-token.png",
                    "alt_text": "Levana Dragons"
                    }
                }
                    ]
                });
                let response = self
                    .client
                    .post(webhook.clone())
                    .json(&value)
                    .send()
                    .await?;
                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "Slack notification POST request failed with code {}",
                        response.status()
                    ))
                }
            }
            None => {
                tracing::warn!("Slack webhook not configured");
                Ok(())
            }
        }
    }

    pub(crate) async fn fetch_market_status(
        &self,
        factories: &[(CosmosNetwork, Address)],
    ) -> anyhow::Result<MarketsConfig> {
        let mut result = MarketsConfig { markets: vec![] };
        for (network, factory) in factories {
            let url = reqwest::Url::parse_with_params(
                "https://querier-mainnet.levana.finance/v1/perps/markets",
                &[
                    ("network", &network.to_string()),
                    ("factory", &factory.to_string()),
                ],
            )?;
            let mut response: MarketsConfig = self
                .client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            result.markets.append(&mut response.markets);
        }
        Ok(result)
    }

    async fn get_internal_market_pair(&self, uri: Url) -> anyhow::Result<CmcExchangeInfo> {
        let result = self
            .client
            .get(uri)
            .header("X-CMC_PRO_API_KEY", &self.cmc_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(result)
    }

    pub(crate) async fn get_exchanges(&self) -> anyhow::Result<Vec<CMCExchange>> {
        let uri = Url::parse("https://pro-api.coinmarketcap.com/v1/exchange/map")?;

        #[derive(serde::Deserialize)]
        struct Result {
            data: Vec<CMCExchange>,
        }

        let result: Result = self
            .client
            .get(uri)
            .header("X-CMC_PRO_API_KEY", &self.cmc_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(result.data)
    }

    pub(crate) async fn get_market_pair(
        &self,
        AssetName(base_asset): AssetName<'_>,
    ) -> anyhow::Result<Vec<CmcMarketPair>> {
        let coin: Coin = base_asset.parse()?;
        let coin = coin.to_wrapped_coin().0;
        let mut start: u32 = 1;
        tracing::debug!("coin id: {}", coin.cmc_id());
        // https://coinmarketcap.com/api/documentation/v1/#operation/getV1ExchangeListingsLatest
        let limit = 5000;
        let uri = |start: u32| {
            Url::parse_with_params(
                "https://pro-api.coinmarketcap.com/v2/cryptocurrency/market-pairs/latest",
                [
                    ("id", coin.cmc_id().to_string().as_str()),
                    ("limit", 5000.to_string().as_str()),
                    ("start", start.to_string().as_str()),
                    ("category", "spot"),
                    ("convert", "usd"),
                ],
            )
        };

        let mut result: CmcExchangeInfo = self
            .client
            .get(uri(start)?)
            .header("X-CMC_PRO_API_KEY", &self.cmc_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let total_exchanges = result.data.num_market_pairs;
        // Ceiled division
        let iterations = (total_exchanges + limit - 1) / limit;

        tracing::debug!("total iterations: {iterations}");

        for index in 0..iterations {
            tracing::debug!("Iteration {index}");
            start += limit;
            let uri = uri(start)?;

            let mut exchange_info = self.get_internal_market_pair(uri).await?;
            result
                .data
                .market_pairs
                .append(&mut exchange_info.data.market_pairs);
        }

        Ok(result.data.market_pairs)
    }

    pub(crate) async fn get_price_in_usd(
        &self,
        NotionalAsset(notional_asset): NotionalAsset<'_>,
    ) -> anyhow::Result<f64> {
        let coin: Coin = notional_asset.parse()?;
        // https://coinmarketcap.com/api/documentation/v1/#operation/getV2CryptocurrencyQuotesLatest
        let id = coin.cmc_id().to_string();
        let uri = Url::parse_with_params(
            "https://pro-api.coinmarketcap.com/v2/cryptocurrency/quotes/latest",
            [("id", id.as_str()), ("convert", "usd")],
        )?;

        #[derive(serde::Deserialize)]
        struct CmcOuter {
            data: HashMap<String, CmcData>,
        }
        #[derive(serde::Deserialize)]
        struct CmcData {
            quote: CmcQuote,
        }
        #[derive(serde::Deserialize)]
        struct CmcQuote {
            #[serde(rename = "USD")]
            usd: CmcQuoteUsd,
        }
        #[derive(serde::Deserialize)]
        struct CmcQuoteUsd {
            price: f64,
        }

        let CmcOuter { mut data } = self
            .client
            .get(uri)
            .header("X-CMC_PRO_API_KEY", &self.cmc_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let CmcData { quote } = data
            .remove(&id)
            .with_context(|| format!("Latest quotes from CMC is missing entry for ID {id}"))?;

        Ok(quote.usd.price)
    }

    pub(crate) async fn get_symbol_map(&self, symbol: &str) -> anyhow::Result<Vec<SymbolMap>> {
        // https://coinmarketcap.com/api/documentation/v1/#operation/getV1CryptocurrencyMap

        let uri = Url::parse_with_params(
            "https://pro-api.coinmarketcap.com/v1/cryptocurrency/map",
            [("symbol", symbol)],
        )?;

        #[derive(serde::Deserialize)]
        struct CmcOuter {
            data: Vec<SymbolMap>,
        }

        let CmcOuter { data } = self
            .client
            .get(uri)
            .header("X-CMC_PRO_API_KEY", &self.cmc_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(data)
    }
}

// We're just using the Debug impl for output, ignore unused fields.
#[allow(dead_code)]
#[derive(serde::Deserialize, Debug)]
pub(crate) struct SymbolMap {
    id: u64,
    name: String,
    symbol: String,
    slug: String,
    first_historical_data: Option<DateTime<Utc>>,
    last_historical_data: Option<DateTime<Utc>>,
}
