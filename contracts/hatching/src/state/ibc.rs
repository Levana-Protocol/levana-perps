use cosmwasm_std::{
    from_binary, IbcChannel, IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcOrder,
    IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg,
};
use msg::contracts::{
    hatching::ibc::{IbcChannelVersion, IbcExecuteMsg},
    ibc_execute::entry::IbcProxyContractMessages,
};
use shared::{
    ibc::{
        ack_success,
        event::{IbcChannelCloseEvent, IbcChannelConnectEvent},
    },
    prelude::*,
};

use super::{nft_mint::NftExecuteMsg, State, StateContext};

impl State<'_> {
    pub(crate) fn handle_ibc_channel_open(&self, msg: IbcChannelOpenMsg) -> Result<()> {
        validate_channel(msg.channel(), msg.counterparty_version())?;
        Ok(())
    }

    pub(crate) fn handle_ibc_channel_connect(
        &mut self,
        ctx: &mut StateContext,
        msg: IbcChannelConnectMsg,
    ) -> Result<()> {
        let version = validate_channel(msg.channel(), msg.counterparty_version())?;

        match version {
            IbcChannelVersion::NftMint => {
                self.config.nft_mint_channel = Some(msg.channel().clone())
            }
            IbcChannelVersion::LvnGrant => {
                self.config.lvn_grant_channel = Some(msg.channel().clone())
            }
        }

        self.save_config(ctx)?;

        ctx.response_mut().add_event(IbcChannelConnectEvent {
            channel: msg.channel(),
        });

        Ok(())
    }

    pub(crate) fn handle_ibc_channel_close(
        &mut self,
        ctx: &mut StateContext,
        msg: IbcChannelCloseMsg,
    ) -> Result<()> {
        // closing an unknown channel shouldn't happen, but if it does, we can treat it as a noop
        if let Ok(version) = IbcChannelVersion::from_str(msg.channel().version.as_str()) {
            match version {
                IbcChannelVersion::NftMint => {
                    self.config.nft_mint_channel = None;
                }
                IbcChannelVersion::LvnGrant => {
                    self.config.lvn_grant_channel = None;
                }
            }

            self.save_config(ctx)?;
        }

        ctx.response_mut().add_event(IbcChannelCloseEvent {
            channel: msg.channel(),
        });
        Ok(())
    }

    pub(crate) fn handle_ibc_packet_receive(
        &self,
        _ctx: &mut StateContext,
        _msg: IbcPacketReceiveMsg,
    ) -> Result<()> {
        bail!("this contract does not support receiving packets");
    }

    pub(crate) fn handle_ibc_packet_ack(
        &self,
        ctx: &mut StateContext,
        ack: IbcPacketAckMsg,
    ) -> Result<()> {
        if ack.acknowledgement.data != ack_success() {
            bail!("packet failed on the other chain");
        }

        if let Ok(msgs) = from_binary::<IbcProxyContractMessages>(&ack.original_packet.data) {
            // there should be at least one message, since an empty list
            // is never sent to the in the first place, but better safe than sorry
            let first_message = msgs.0.get(0).context("no ibc proxy contract messages")?;
            // get the hatch id from the first minted NFT. They're all the same
            match from_binary::<NftExecuteMsg>(first_message)? {
                NftExecuteMsg::Mint(msg) => {
                    self.update_hatch_status(ctx, msg.extract_hatch_id()?, |mut status| {
                        status.nft_mint_completed = true;
                        Ok(status)
                    })?;
                }
            }
        } else if let Ok(msg) = from_binary::<IbcExecuteMsg>(&ack.original_packet.data) {
            match msg {
                IbcExecuteMsg::GrantLvn { hatch_id, .. } => {
                    self.update_hatch_status(ctx, hatch_id.parse()?, |mut status| {
                        status.lvn_grant_completed = true;
                        Ok(status)
                    })?;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn handle_ibc_packet_timeout(&self, _msg: IbcPacketTimeoutMsg) -> Result<()> {
        // This is called if the relayer detects a timeout
        Ok(())
    }
}

pub fn validate_channel(
    channel: &IbcChannel,
    counterparty_version: Option<&str>,
) -> Result<IbcChannelVersion> {
    let version = IbcChannelVersion::from_str(&channel.version)?;

    if let Some(counterparty_version) = counterparty_version {
        let counterparty_version = IbcChannelVersion::from_str(counterparty_version)?;
        if counterparty_version != version {
            bail!(
                "counterparty version {:?} is different than channel version {:?}",
                counterparty_version,
                version
            );
        }
    }

    if channel.order != IbcOrder::Unordered {
        bail!(
            "Expected ibc channel ordering to be {:?}, but instead it's {:?}",
            IbcOrder::Unordered,
            channel.order
        );
    }

    Ok(version)
}
