use cosmwasm_std::{
    from_binary, IbcChannel, IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcOrder,
    IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg,
};
use msg::contracts::hatching::ibc::{IbcChannelVersion, IbcExecuteMsg};
use shared::{
    ibc::{
        ack_success,
        event::{IbcChannelCloseEvent, IbcChannelConnectEvent},
    },
    prelude::*,
};

use super::{State, StateContext};

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
        let channel = validate_channel(msg.channel(), msg.counterparty_version())?;

        match channel {
            IbcChannelVersion::LvnGrant => {
                self.config.lvn_grant_channel = Some(msg.channel().clone());
                self.save_config(ctx.storage)?;
            }
            _ => bail!("Unknown channel: {:?}", channel),
        }

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
        if let Ok(channel) = IbcChannelVersion::from_str(msg.channel().version.as_str()) {
            match channel {
                IbcChannelVersion::LvnGrant => {
                    self.config.lvn_grant_channel = None;
                }
                _ => bail!("Unknown channel: {:?}", channel),
            }

            self.save_config(ctx.storage)?;
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
        from_binary(&msg.packet.data)
            .map_err(|err| err.into())
            .and_then(|msg| {
                match msg {
                    IbcExecuteMsg::GrantLvn {
                        address, amount, ..
                    } => {
                        let address = self.api.addr_validate(&address)?;
                        let amount =
                            NonZero::<LvnToken>::try_from_decimal(amount.into_decimal256())
                                .with_context(|| "unable to convert rewards into LvnToken")?;
                        self.grant_rewards(ctx, address, amount)?
                    }
                }
                Ok(())
            })
    }

    pub(crate) fn handle_ibc_packet_ack(
        &self,
        _ctx: &mut StateContext,
        ack: IbcPacketAckMsg,
    ) -> Result<()> {
        if ack.acknowledgement.data != ack_success() {
            bail!("packet failed on the other chain");
        }

        Ok(())
    }

    pub(crate) fn handle_ibc_packet_timeout(&self, _msg: IbcPacketTimeoutMsg) -> Result<()> {
        // This is called if the relayer detects a timeout
        Ok(())
    }
}

pub(crate) fn validate_channel(
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
