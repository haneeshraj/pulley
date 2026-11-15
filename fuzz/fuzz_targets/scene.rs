#![no_main]
// See pulley_core::fuzz::scene for what this checks.
libfuzzer_sys::fuzz_target!(|data: &[u8]| pulley_core::fuzz::scene(data));
