use cosmwasm_std::{
    from_binary, IbcChannel, IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcOrder,
    IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg,
};
use msg::contracts::ibc_execute_proxy::entry::{IbcProxyContractMessages, InstantiateMsg};
use serde::{Deserialize, Serialize};
use shared::{
    ibc::event::{IbcChannelCloseEvent, IbcChannelConnectEvent},
    prelude::*,
};

use super::{State, StateContext};

const EXPECTED_CHANNEL_INFO: Item<ExpectedChannelInfo> = Item::new("expected_channel_info");

#[derive(Serialize, Deserialize, Debug)]
struct ExpectedChannelInfo {
    pub(crate) version: String,
    pub(crate) order: IbcOrder,
}

impl State<'_> {
    pub(crate) fn validate_channel(
        &self,
        storage: &dyn Storage,
        channel: &IbcChannel,
        counterparty_version: Option<&str>,
    ) -> Result<()> {
        let expected = EXPECTED_CHANNEL_INFO.load(storage)?;

        if channel.version != expected.version {
            bail!(
                "channel version {:?} is different than expected version {:?}",
                counterparty_version,
                expected.version
            );
        }

        if let Some(counterparty_version) = counterparty_version {
            if counterparty_version != expected.version {
                bail!(
                    "counterparty version {:?} is different than channel version {:?}",
                    counterparty_version,
                    expected.version
                );
            }
        }

        if channel.order != expected.order {
            bail!(
                "channel ordering {:?} is different than expected ordering {:?}",
                channel.order,
                expected.order
            );
        }

        Ok(())
    }

    pub(crate) fn handle_ibc_channel_open(
        &self,
        storage: &dyn Storage,
        msg: IbcChannelOpenMsg,
    ) -> Result<()> {
        self.validate_channel(storage, msg.channel(), msg.counterparty_version())
    }

    pub(crate) fn handle_ibc_channel_connect(
        &mut self,
        ctx: &mut StateContext,
        msg: IbcChannelConnectMsg,
    ) -> Result<()> {
        self.validate_channel(ctx.storage, msg.channel(), msg.counterparty_version())?;

        self.config.ibc_channel = Some(msg.channel().clone());
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
        if let Some(expected) = EXPECTED_CHANNEL_INFO.may_load(ctx.storage)? {
            // closing an unknown channel shouldn't happen, but if it does, we can treat it as a noop
            if expected.version == msg.channel().version {
                self.config.ibc_channel = None;
                self.save_config(ctx)?;
            }
        }

        ctx.response_mut().add_event(IbcChannelCloseEvent {
            channel: msg.channel(),
        });
        Ok(())
    }

    pub(crate) fn handle_ibc_packet_receive(
        &self,
        ctx: &mut StateContext,
        msg: IbcPacketReceiveMsg,
    ) -> Result<()> {
        // The other side *must* send this data type
        let msgs: IbcProxyContractMessages = from_binary(&msg.packet.data)?;

        self.send(ctx, msgs.0)?;

        Ok(())
    }

    pub(crate) fn handle_ibc_packet_ack(
        &self,
        _ctx: &mut StateContext,
        _ack: IbcPacketAckMsg,
    ) -> Result<()> {
        // This is called when the other side acknowledges a packet
        Ok(())
    }

    pub(crate) fn handle_ibc_packet_timeout(&self, _msg: IbcPacketTimeoutMsg) -> Result<()> {
        // This is called if the relayer detects a timeout
        Ok(())
    }
}

pub(crate) fn init_ibc(store: &mut dyn Storage, msg: &InstantiateMsg) -> Result<()> {
    EXPECTED_CHANNEL_INFO.save(
        store,
        &ExpectedChannelInfo {
            version: msg.ibc_channel_version.clone(),
            order: msg.ibc_channel_order.clone(),
        },
    )?;

    Ok(())
}
