use super::{State, StateContext};
use cosmwasm_std::{to_binary, IbcMsg, IbcTimeout};
use msg::contracts::hatching::{ibc::IbcExecuteMsg, HatchDetails};
use shared::{ibc::TIMEOUT_SECONDS, prelude::*};

impl State<'_> {
    pub(crate) fn send_grant_lvn_ibc_message(
        &self,
        ctx: &mut StateContext,
        hatch_id: u64,
        lvn_grant_address: String,
        amount: NumberGtZero,
    ) -> Result<()> {
        // outbound IBC message, where packet is then received on other chain
        let channel_id = self
            .config
            .lvn_grant_channel
            .as_ref()
            .context("no lvn grant channel")?
            .endpoint
            .channel_id
            .clone();

        let msg = IbcExecuteMsg::GrantLvn {
            address: lvn_grant_address,
            amount,
            hatch_id: hatch_id.to_string(),
        };

        ctx.response_mut().add_message(IbcMsg::SendPacket {
            channel_id,
            data: to_binary(&msg)?,
            timeout: IbcTimeout::with_timestamp(self.env.block.time.plus_seconds(TIMEOUT_SECONDS)),
        });

        Ok(())
    }
}

// if the amount is not greater than zero, returns Ok(None)
pub fn get_lvn_to_grant(details: &HatchDetails) -> Result<Option<NumberGtZero>> {
    let mut total: Decimal256 = Decimal256::zero();

    for egg in &details.eggs {
        total = total.checked_add(egg.lvn.into_decimal256())?;
    }

    for dust in &details.dusts {
        total = total.checked_add(dust.lvn.into_decimal256())?;
    }

    Ok(NumberGtZero::new(total))
}
