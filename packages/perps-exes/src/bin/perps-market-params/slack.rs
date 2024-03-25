use anyhow::anyhow;
use cosmos::{Address, CosmosNetwork};
use reqwest::{Client, Url};
use shared::storage::MarketId;

use crate::{
    coingecko::{CmcExchangeInfo, Coin},
    market_param::MarketsConfig,
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

    pub(crate) async fn get_market_pair(
        &self,
        market_id: MarketId,
    ) -> anyhow::Result<CmcExchangeInfo> {
        let base_asset = market_id.get_base();
        let coin: Coin = base_asset.parse()?;
        let mut start: u32 = 1;
        // https://coinmarketcap.com/api/documentation/v1/#operation/getV1ExchangeListingsLatest
        let limit = 5000;
        let uri = |start: u32| {
            Url::parse_with_params(
                "https://pro-api.coinmarketcap.com/v1/exchange/listings/latest",
                [
                    ("id", coin.cmc_id().to_string().as_str()),
                    ("limit", 5000.to_string().as_str()),
                    ("start", start.to_string().as_str()),
                    ("category", "spot"),
                    ("centerType", "cex"),
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

        tracing::info!("total iterations: {iterations}");

        for index in 0..iterations {
            tracing::info!("Iteration {index}");
            start += limit;
            let uri = uri(start)?;

            let mut exchange_info = self.get_internal_market_pair(uri).await?;
            result
                .data
                .market_pairs
                .append(&mut exchange_info.data.market_pairs);
        }

        Ok(result)
    }
}
