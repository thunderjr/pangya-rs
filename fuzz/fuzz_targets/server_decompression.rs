#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| { let _=pangya_crypto::server_decrypt(data,data.first().copied().unwrap_or(0)&0x0f,1024*1024,128); });
