//! Implements the logic for capping funding payments based on aggregate values.
//!
//! The goal is to provide standalone logic in this module that can be easily
//! unit tested on its own.

use crate::prelude::*;

/// Result of the capping procedure.
#[derive(PartialEq, Eq, Debug)]
pub(super) enum AggregateCapping {
    /// No capping occured, position should pay this much (positive) or receive it (negative).
    NoCapping,
    /// Cap the amount paid
    Capped { capped_amount: Signed<Collateral> },
}

/// Cap a funding payment based on protocol totals and position information.
///
/// See comments inline within the function for an explanation of the procedure.
pub(super) fn aggregate_capping(
    total_paid: Signed<Collateral>,
    total_margin: Collateral,
    amount: Signed<Collateral>,
    pos_margin: Collateral,
) -> anyhow::Result<AggregateCapping> {
    let mut is_capped = false;

    // We need to ensure that, even without this position's margin, we have
    // enough margin available to cover the requested payment. So first, get the
    // total margin without this position's margin.
    let total_margin_without_pos = total_margin
        .checked_sub(pos_margin)
        .context("Total margin is invalid, less than position's margin")?;

    // Next, calculate the available margin for payments by including the total
    // paid with this value. Since a positive value for total_paid means money
    // entering the system, and negative means money leaving, the available
    // margin is the sum of these two values.
    let available_margin = total_margin_without_pos
        .into_signed()
        .checked_add(total_paid)?;

    // The available margin is positive to represent funds available for an
    // outgoing payment. However, amount is _negative_ to represent that. We
    // want to be able to compare these directly for magnitude, so switch the
    // signs on margin so that a negative value _also_ means "can pay out from
    // the protocol to the trader."
    //
    // Note that this new value can be positive, which means "the trader must
    // pay into the protocol to keep solvency."
    let available_margin = -available_margin;

    // Now we can perform our first capping: we cannot send more to the trader
    // than the available margin. Since outgoing payments are negative, that
    // means that amount must be greater than or equal to available margin.
    let capped_amount = if amount < available_margin {
        is_capped = true;
        available_margin
    } else {
        amount
    };

    // Next, we need to ensure that the trader doesn't pay more into the system
    // than the available margin. For this, we want to keep the position margin
    // as a positive value to represent "greatest possible outgoing payment."
    let pos_margin = pos_margin.into_signed();

    // Now enforce that the amount is, at most, the position margin.
    let capped_amount = if capped_amount > pos_margin {
        is_capped = true;
        pos_margin
    } else {
        capped_amount
    };

    // And finally return a value.
    Ok(if is_capped {
        AggregateCapping::Capped { capped_amount }
    } else {
        AggregateCapping::NoCapping
    })
}

#[cfg(test)]
mod tests {
    use super::{aggregate_capping, AggregateCapping};

    fn helper(
        total_paid: &str,
        total_margin: &str,
        amount: &str,
        pos_margin: &str,
    ) -> AggregateCapping {
        aggregate_capping(
            total_paid.parse().unwrap(),
            total_margin.parse().unwrap(),
            amount.parse().unwrap(),
            pos_margin.parse().unwrap(),
        )
        .unwrap()
    }

    fn no_capping() -> AggregateCapping {
        AggregateCapping::NoCapping
    }

    fn capped(capped_amount: &str) -> AggregateCapping {
        AggregateCapping::Capped {
            capped_amount: capped_amount.parse().unwrap(),
        }
    }

    #[test]
    fn no_capping_trader_pays() {
        assert_eq!(helper("5", "10", "2", "3"), no_capping());
    }

    #[test]
    fn no_capping_trader_receives() {
        assert_eq!(helper("5", "10", "-2", "3"), no_capping());
    }

    #[test]
    fn capped_trader_receives() {
        // Trader expects to receive 2. The total margin will be 13 - 3 == 10.
        // But we've already paid our 9. Therefore, we only have 1 more to pay
        // out.
        assert_eq!(helper("-9", "13", "-2", "3"), capped("-1"));
    }

    #[test]
    fn capped_trader_pays() {
        // Trader should pay only 1. However, margin will end up being 10 - 3 ==
        // 7, leaving a gap of 9 - 7 == 2. Therefore, we pay 2 instead.
        assert_eq!(helper("-9", "10", "1", "3"), capped("2"));
    }

    #[test]
    fn capped_trader_pays_insufficient_pos_margin() {
        // We're already insolvent! And we don't have enough margin on the
        // position to cover the insolvency. We take the maximum available from
        // the position.
        assert_eq!(helper("-9", "8", "1", "2"), capped("2"));
    }

    #[test]
    fn uncapped_trader_positive_payments() {
        // It looks like we have insufficient margin, but actually the total net
        // payments are covering us.
        assert_eq!(helper("5", "2", "-5", "2"), no_capping());
    }

    #[test]
    fn double_cap() {
        // In this case we get capped on the total (since we're currently
        // insolvent and want to be paid into the protocol), but we have no
        // liquidation margin, so we go back to 0. We still want to count this
        // as a capping even though we're making the original payment amount.
        assert_eq!(helper("-5", "4", "0", "0"), capped("0"));
    }
}
