use crate::state::config::lvn_from_profile_spirit_level;

use super::{State, StateContext};
use msg::contracts::hatching::ProfileInfo;
use serde::{Deserialize, Serialize};
use shared::prelude::*;

impl State<'_> {
    pub(crate) fn drain_profile(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
    ) -> Result<Option<ProfileInfo>> {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ProfileQueryMsg {
            GetSpiritLevel { addr: String },
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub struct GetSpiritLevelResponse {
            pub spirit_level: String,
        }

        let resp: GetSpiritLevelResponse = self.querier.query_wasm_smart(
            self.config.profile_contract.clone(),
            &ProfileQueryMsg::GetSpiritLevel {
                addr: owner.to_string(),
            },
        )?;

        if let Ok(spirit_level) = NumberGtZero::from_str(&resp.spirit_level) {
            // remove spirit level from profile contract
            #[derive(Serialize, Deserialize)]
            #[serde(rename_all = "snake_case")]
            enum ProfileExecuteMsg {
                Admin { message: ProfileAdminExecuteMsg },
            }
            #[derive(Serialize, Deserialize)]
            #[serde(rename_all = "snake_case")]
            pub enum ProfileAdminExecuteMsg {
                RemoveSpiritLevel { wallets: Vec<RemoveSpiritLevel> },
            }

            #[derive(Serialize, Deserialize)]
            #[serde(rename_all = "snake_case")]
            pub struct RemoveSpiritLevel {
                pub wallet: String,
                pub spirit_level: Option<String>,
            }
            ctx.response_mut().add_execute_submessage_oneshot(
                self.config.profile_contract.clone(),
                &ProfileExecuteMsg::Admin {
                    message: ProfileAdminExecuteMsg::RemoveSpiritLevel {
                        wallets: vec![RemoveSpiritLevel {
                            wallet: owner.to_string(),
                            spirit_level: Some(spirit_level.to_string()),
                        }],
                    },
                },
            )?;

            // calculate lvn from spirit level
            let lvn = lvn_from_profile_spirit_level(spirit_level)?;
            Ok(Some(ProfileInfo { spirit_level, lvn }))
        } else {
            Ok(None)
        }
    }
}
