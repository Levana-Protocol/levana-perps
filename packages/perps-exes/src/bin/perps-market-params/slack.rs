use anyhow::anyhow;
use reqwest::{blocking::Client, Url};

pub(crate) struct SlackApp {
    webhook: Url,
    client: Client,
}

impl SlackApp {
    pub(crate) fn new(webhook: Url) -> Self {
        let client = Client::new();
        SlackApp { webhook, client }
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
}
