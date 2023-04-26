use wasm_bindgen_futures::{spawn_local, JsFuture};

use super::state::*;
use crate::{page::home::app::ActionHistory, prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKind {
    All,
    StripEvents,
}

impl EventView {
    pub fn copy_to_clipboard(&self, kind: CopyKind) {
        let mut data: Vec<ActionHistory> = self.stats.action_history.lock_ref().to_vec();

        if kind == CopyKind::StripEvents {
            for data in &mut data {
                match data {
                    ActionHistory::Tx(tx) => {
                        tx.events = vec![];
                    }
                    _ => {}
                }
            }
        }

        let mut text = serde_json::to_string_pretty(&data).unwrap();

        #[cfg(web_sys_unstable_apis)]
        {
            spawn_local(async move {
                let fut = web_sys::window()
                    .unwrap_ext()
                    .navigator()
                    .clipboard()
                    .unwrap_ext()
                    .write_text(&text);

                JsFuture::from(fut).await.unwrap_ext();
                log::info!("Copied to clipboard");
            });
        }
        #[cfg(not(web_sys_unstable_apis))]
        {
            log::error!("Cannot copy to clipboard, enable web_sys_unstable_apis cfg");
        }
    }
}
