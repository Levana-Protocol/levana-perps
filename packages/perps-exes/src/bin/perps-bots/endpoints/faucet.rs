use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{extract::State, Json};
use cosmos::Address;

use crate::app::{faucet::FaucetTapError, App};

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
    match app.is_valid_recaptcha(&hcaptcha).await {
        Ok(true) => (),
        Ok(false) => return Err(FaucetTapError::InvalidCaptcha {}),
        Err(_) => return Err(FaucetTapError::CannotQueryCaptcha {}),
    }
    match &app.faucet_bot {
        Some(faucet_bot) => faucet_bot.tap(app, recipient, cw20s).await,
        None => Err(FaucetTapError::Mainnet {}),
    }
}

impl App {
    pub(crate) async fn is_valid_recaptcha(&self, g_recaptcha_response: &str) -> Result<bool> {
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
                secret: self
                    .faucet_bot
                    .as_ref()
                    .context("No faucet on mainnet")?
                    .get_hcaptcha_secret(),
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
