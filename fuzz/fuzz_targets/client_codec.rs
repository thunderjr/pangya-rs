#![no_main]
use bytes::BytesMut;
use libfuzzer_sys::fuzz_target;
use pangya_protocol::{CodecLimits, FrameCodec, ServiceKind};
use tokio_util::codec::Decoder;
fuzz_target!(|data: &[u8]| { let mut codec=FrameCodec::new(data.first().copied().unwrap_or(0)&0x0f,ServiceKind::Login,CodecLimits{max_client_frame_bytes:65_535,max_server_plaintext_bytes:1024,max_expansion_ratio:128}); let mut bytes=BytesMut::from(data); while matches!(codec.decode(&mut bytes),Ok(Some(_))) {} });
