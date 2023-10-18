use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, Json};
use cosmos::Address;

use crate::{
    app::{faucet::FaucetTapError, App},
    config::{BotConfigByType, BotConfigTestnet},
};

#[derive(serde::Deserialize)]
pub(crate) struct FaucetQuery {
    cw20s: Vec<Address>,
    recipient: Address,
    #[serde(rename = "hCaptcha")]
    hcaptcha: String,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FaucetResponse {
    Success {
        message: String,
        txhash: String,
    },
    Error {
        message: String,
        error: FaucetTapError,
    },
}

pub(crate) async fn bot(
    State(app): State<Arc<App>>,
    Json(query): Json<FaucetQuery>,
) -> Json<FaucetResponse> {
    Json(match bot_inner(&app, query).await {
        Ok(txhash) => FaucetResponse::Success {
            message: format!("Faucet successfully tapped in {txhash}"),
            txhash: txhash.to_string(),
        },
        Err(e) => FaucetResponse::Error {
            message: e.to_string(),
            error: e,
        },
    })
}

async fn bot_inner(
    app: &App,
    FaucetQuery {
        cw20s,
        recipient,
        hcaptcha,
    }: FaucetQuery,
) -> Result<Arc<String>, FaucetTapError> {
    match &app.config.by_type {
        BotConfigByType::Mainnet { .. } => Err(FaucetTapError::Mainnet {}),
        BotConfigByType::Testnet { inner } => {
            match app.is_valid_recaptcha(&hcaptcha, inner).await {
                Ok(true) => inner.faucet_bot.tap(app, recipient, cw20s).await,
                Ok(false) => Err(FaucetTapError::InvalidCaptcha {}),
                Err(e) => {
                    tracing::error!("Cannot query captcha service: {e:?}");
                    Err(FaucetTapError::CannotQueryCaptcha {})
                }
            }
        }
    }
}

impl App {
    pub(crate) async fn is_valid_recaptcha(
        &self,
        g_recaptcha_response: &str,
        testnet: &BotConfigTestnet,
    ) -> Result<bool> {
        #[derive(serde::Serialize)]
        struct Body<'a> {
            secret: &'a str,
            response: &'a str,
        }
        #[derive(serde::Deserialize)]
        struct Res {
            success: bool,
        }
        let Res { success } = self
            .client
            .post("https://hcaptcha.com/siteverify")
            .form(&Body {
                secret: testnet.faucet_bot.get_hcaptcha_secret(),
                response: g_recaptcha_response,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(success)
    }
}
