use anyhow::anyhow;
use cosmos::{Address, CosmosNetwork};
use reqwest::{blocking::Client, Url};

use crate::market_param::MarketsConfig;

pub(crate) struct HttpApp {
    webhook: Url,
    client: Client,
}

impl HttpApp {
    pub(crate) fn new(webhook: Url) -> Self {
        let client = Client::new();
        HttpApp { webhook, client }
    }

    pub(crate) fn send_notification(
        &self,
        message: String,
        description: String,
    ) -> anyhow::Result<()> {
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
        let response = self.client.post(self.webhook.clone()).json(&value).send()?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow!(
                "Slack notification POST request failed with code {}",
                response.status()
            ))
        }
    }

    pub(crate) fn fetch_market_status(
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
            let mut response: MarketsConfig =
                self.client.get(url).send()?.error_for_status()?.json()?;
            result.markets.append(&mut response.markets);
        }
        Ok(result)
    }
}
