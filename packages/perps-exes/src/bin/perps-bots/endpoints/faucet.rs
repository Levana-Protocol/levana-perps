use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, Json};
use cosmos::Address;
use msg::prelude::PerpError;
use serde_json::de::StrRead;

use crate::app::App;

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
    State(app): State<Arc<App>>,
    Json(query): Json<FaucetQuery>,
) -> Json<FaucetResponse> {
    Json(match bot_inner(&app, query).await {
        Ok(x) => x,
        Err(e) => {
            log::error!("Faucet tap failed: {e:?}");
            FaucetResponse::Error {
                message: e
                    .downcast_ref::<tonic::Status>()
                    .and_then(|status| parse_perp_error(status.message()))
                    .map(|perp_error| perp_error.description)
                    .unwrap_or_else(|| e.to_string()),
            }
        }
    })
}

// todo - this isn't only part of faucet, is used elsewhere too
pub fn parse_perp_error(err: &str) -> Option<PerpError<serde_json::Value>> {
    // This weird parsing to (1) strip off the content before the JSON body
    // itself and (2) ignore the trailing data after the JSON data
    let start = err.find(" {")?;
    let err = &err[start..];
    serde_json::Deserializer::new(StrRead::new(err))
        .into_iter()
        .next()?
        .ok()
}

#[cfg(test)]
mod tests {
    use msg::prelude::{ErrorDomain, ErrorId, PerpError};

    use super::*;

    #[test]
    fn test_parse_perp_error() {
        const INPUT: &str = "failed to execute message; message index: 0: {\n  \"id\": \"exceeded\",\n  \"domain\": \"faucet\",\n  \"description\": \"exceeded tap limit, wait 284911 more seconds\",\n  \"data\": {\n    \"wait_secs\": \"284911\"\n  }\n}: execute wasm contract failed [CosmWasm/wasmd@v0.29.2/x/wasm/keeper/keeper.go:425] With gas wanted: '0' and gas used: '115538' ";
        let expected = PerpError {
            id: ErrorId::Exceeded,
            domain: ErrorDomain::Faucet,
            description: "exceeded tap limit, wait 284911 more seconds".to_owned(),
            data: None,
        };
        let mut actual = parse_perp_error(INPUT).unwrap();
        actual.data = None;
        assert_eq!(actual, expected);
    }
}

async fn bot_inner(
    app: &App,
    FaucetQuery {
        cw20s,
        recipient,
        hcaptcha,
    }: FaucetQuery,
) -> Result<FaucetResponse> {
    if !app.is_valid_recaptcha(&hcaptcha).await? {
        return Ok(FaucetResponse::Error {
            message: "Invalid hCaptcha".to_owned(),
        });
    }
    let txhash = app.faucet_bot.tap(app, recipient, cw20s).await?;
    Ok(FaucetResponse::Success {
        message: format!("Successfully tapped faucet in txhash {}", txhash),
    })
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
                secret: self.faucet_bot.get_hcaptcha_secret(),
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
