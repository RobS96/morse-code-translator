#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| morse_core::fuzzing::schedule(data));
