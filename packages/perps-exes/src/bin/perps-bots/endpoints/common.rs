use tracing::{instrument, span, Level, info_span};

pub(crate) async fn homepage() -> &'static str {
    let tx_ctx = sentry::TransactionContext::new("homepage", "homepage");
    let transaction = sentry::start_transaction(tx_ctx);

    // Transactions can have child spans, and those spans can have child spans as well.
    let span = transaction.start_child("homepage", "Home page render");

    std::thread::sleep(std::time::Duration::from_millis(50));

    span.finish(); // Remember that only finished spans will be sent with the transaction
    transaction.finish();

    r#"Welcome intrepid reader!

Not sure what you thought you'd find, but you didn't find it.

Better luck next time."#
}

#[instrument(skip_all, name="health_perps")]
pub(crate) async fn healthz() -> &'static str {
    "Yup, I'm alive"
}

#[instrument(skip_all, name="build_perps")]
pub(crate) async fn build_version() -> &'static str {
    perps_exes::build_version()
}
