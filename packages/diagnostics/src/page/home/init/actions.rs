use super::{InitUi, Phase};
use crate::prelude::*;
use msg::{
    contracts::market::{
        config::ConfigUpdate,
        entry::{ExecuteMsg, ExecuteOwnerMsg, QueryMsg, StatusResp},
    },
    token::Token,
};
use wasm_bindgen_futures::spawn_local;

impl InitUi {
    pub fn connect(self: Rc<Self>) {
        let state = self;

        state.phase.set(Phase::Connecting);

        spawn_local(clone!(state => async move {
            let bridge = match crate::bridge::Bridge::connect().await {
                Ok(bridge) => {
                    bridge.exec_market(ExecuteMsg::Owner(ExecuteOwnerMsg::ConfigUpdate {
                        update:  Box::new(ConfigUpdate{
                            crank_fee_charged: Some("0.01".parse().unwrap()),
                            crank_fee_reward: Some("0.001".parse().unwrap()),
                            ..Default::default()
                        })
                    }), None).await.unwrap();

                    let resp = bridge.query_market::<StatusResp>(QueryMsg::Status { price:None }).await.unwrap();
                    state.phase.set(Phase::Connected{bridge, status: resp.data});
                }
                Err(err) => {
                    state.phase.set(Phase::Error(err.to_string()));
                }
            };
        }));
    }
}
