//! Ibc helpers
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary};

/// Timeout in seconds for IBC packets
pub const TIMEOUT_SECONDS: u64 = 60 * 10; // 10 minutes

/// IBC ACK. See:
/// https://github.com/cosmos/cosmos-sdk/blob/f999b1ff05a4db4a338a855713864497bedd4396/proto/ibc/core/channel/v1/channel.proto#L141-L147
#[cw_serde]
enum Ack {
    Result(Binary),
    Error(String),
}

/// IBC ACK success
pub fn ack_success() -> Binary {
    to_json_binary(&Ack::Result(b"1".into())).unwrap()
}

/// IBC ACK failure
pub fn ack_fail(err: anyhow::Error) -> Binary {
    to_json_binary(&Ack::Error(err.to_string())).unwrap()
}

/// Common IBC Events
pub mod event {
    use cosmwasm_std::{Event, IbcChannel};

    /// IBC Channel Connect Event
    #[derive(Debug)]
    pub struct IbcChannelConnectEvent<'a> {
        /// The IBC channel
        pub channel: &'a IbcChannel,
    }

    impl From<IbcChannelConnectEvent<'_>> for Event {
        fn from(src: IbcChannelConnectEvent) -> Self {
            mixin_ibc_channel(Event::new("ibc-channel-connect"), src.channel)
        }
    }

    /// IBC Channel Close Event
    #[derive(Debug)]
    pub struct IbcChannelCloseEvent<'a> {
        /// The IBC channel
        pub channel: &'a IbcChannel,
    }

    impl From<IbcChannelCloseEvent<'_>> for Event {
        fn from(src: IbcChannelCloseEvent) -> Self {
            mixin_ibc_channel(Event::new("ibc-channel-close"), src.channel)
        }
    }

    fn mixin_ibc_channel(event: Event, channel: &IbcChannel) -> Event {
        event
            .add_attribute("endpoint-id", &channel.endpoint.channel_id)
            .add_attribute("endpoint-port-id", &channel.endpoint.port_id)
            .add_attribute(
                "counterparty-endpoint-id",
                &channel.counterparty_endpoint.channel_id,
            )
            .add_attribute(
                "counterparty-endpoint-port-id",
                &channel.counterparty_endpoint.port_id,
            )
            .add_attribute("order", format!("{:?}", channel.order))
            .add_attribute("version", &channel.version)
            .add_attribute("connection-id", &channel.connection_id)
    }
}
