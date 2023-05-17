pub(super) fn unpend_limit() -> u32 {
    super::Config::default().unpend_limit
}

pub(super) fn liquifunding_delay_fuzz_seconds() -> u32 {
    super::Config::default().liquifunding_delay_fuzz_seconds
}
