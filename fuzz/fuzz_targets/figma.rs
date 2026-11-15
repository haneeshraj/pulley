#![no_main]
// See pulley_core::fuzz::figma for what this checks.
libfuzzer_sys::fuzz_target!(|data: &[u8]| pulley_core::fuzz::figma(data));
