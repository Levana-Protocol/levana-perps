// (un)comment these as needed for a given debugging session
#[cfg(feature = "debug_log")]
static DEBUG_LOG_FLAGS: &[DebugLog] = &[];

/// Flags for gating debug_log
#[allow(missing_docs)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DebugLog {
    SanityFundsAddUnallocated,
    SanityFundsRemoveUnallocated,
    SanityFundsAddCollateral,
    SanityFundsRemoveCollateral,
    SanityFundsAddTradingFees,
    SanityFundsAddBorrowFees,
    SanityFundsRemoveFees,
    SanityFundsAddLiquidity,
    SanityFundsRemoveLiquidity,
    SanityFundsBalanceAssertion,
    SanityFundsSubtotal,
    SanityFundsDeltaNeutralityFee,
    FundingPaymentEvent,
    FundingRateChangeEvent,
    BorrowFeeEvent,
    TradingFeeEvent,
    DeltaNeutralityFeeEvent,
    LimitOrderFeeEvent,
    DeltaNeutralityRatioEvent,
}

/// internal-only, used by the macros
#[cfg(feature = "debug_log")]
pub fn debug_log_inner(flag: Option<DebugLog>, s: &str) {
    let should_print = match &flag {
        None => true,
        Some(flag) => DEBUG_LOG_FLAGS.contains(flag),
    };

    if should_print {
        println!("{}", s);
    }
}

/// This version will always log
/// (except if log_print feature is not enabled, it's a no-op)
#[cfg(feature = "debug_log")]
#[macro_export]
macro_rules! debug_log_any {
    ($($t:tt)*) => {{
        $crate::log::debug_log_inner(None, &format!($($t)*));
    }};

}

/// This version only logs if the given flag is in DEBUG_LOG_FLAGS
/// (will not log if log_print feature is disabled, in that case it's a no-op)
#[cfg(feature = "debug_log")]
#[macro_export]
macro_rules! debug_log {
    ($flag:expr, $($t:tt)*) => {{
        $crate::log::debug_log_inner(Some($flag), &format!($($t)*));
    }};
}

/// This version will always log
/// (except if log_print feature is not enabled, it's a no-op)
#[cfg(not(feature = "debug_log"))]
#[macro_export]
macro_rules! debug_log_any {
    ($($t:tt)*) => {{}};
}

/// This version only logs if the given flag is in DEBUG_LOG_FLAGS
/// (will not log if log_print feature is disabled, in that case it's a no-op)
#[cfg(not(feature = "debug_log"))]
#[macro_export]
macro_rules! debug_log {
    ($flag:expr, $($t:tt)*) => {{}};
}
