#![deny(clippy::as_conversions)]
pub mod constants;
pub mod contract;
mod deferred_exec;
pub(crate) mod prelude;
pub mod state;

/// Injects failure in dev, no-op in prod.
#[cfg(debug_assertions)]
pub fn inject_failures_during_test() -> anyhow::Result<()> {
    let env = std::env::var("LEVANA_CONTRACTS_INJECT_FAILURE");
    if env.is_ok() {
        anyhow::bail!("Injected failure during testing")
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
pub fn inject_failures_during_test() -> anyhow::Result<()> {
    Ok(())
}
