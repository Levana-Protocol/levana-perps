pub(crate) async fn homepage() -> &'static str {
    r#"Welcome intrepid reader!
    
Not sure what you thought you'd find, but you didn't find it.

Better luck next time."#
}

pub(crate) async fn healthz() -> &'static str {
    "Yup, I'm alive"
}

pub(crate) async fn build_version() -> &'static str {
    perps_exes::build_version()
}
