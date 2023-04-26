use anyhow::Result;
use axum::{Extension, Json};
use cosmos::{Address, HasAddress, HasCosmos};
use msg::contracts::faucet::entry::{ExecuteMsg, FaucetAsset};

use crate::app::{App, FaucetBot};

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
    Success { message: String },
    Error { message: String },
}

pub(crate) async fn bot(
    Extension(app): Extension<App>,
    Json(query): Json<FaucetQuery>,
) -> Json<FaucetResponse> {
    Json(match bot_inner(&app, query).await {
        Ok(x) => x,
        Err(e) => {
            log::error!("Faucet tap failed: {e:?}");
            FaucetResponse::Error {
                message: e.to_string(),
            }
        }
    })
}

async fn bot_inner(app: &App, query: FaucetQuery) -> Result<FaucetResponse> {
    if !app.faucet_bot.is_valid_recaptcha(&query.hcaptcha).await? {
        return Ok(FaucetResponse::Error {
            message: "Invalid hCaptcha".to_owned(),
        });
    }
    let res = app
        .faucet_bot
        .faucet
        .execute(
            &app.faucet_bot.wallet,
            vec![],
            ExecuteMsg::Tap {
                assets: query
                    .cw20s
                    .into_iter()
                    .map(|x| FaucetAsset::Cw20(x.get_address_string().into()))
                    .chain(std::iter::once(FaucetAsset::Native(
                        app.faucet_bot.faucet.get_cosmos().get_gas_coin().clone(),
                    )))
                    .collect(),
                recipient: query.recipient.get_address_string().into(),
                amount: None,
            },
        )
        .await?;
    Ok(FaucetResponse::Success {
        message: format!("Successfully tapped faucet in txhash {}", res.txhash),
    })
}

impl FaucetBot {
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
                secret: &self.hcaptcha_secret,
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
