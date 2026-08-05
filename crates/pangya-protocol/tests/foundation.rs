//! M1 protocol foundation integration tests.

use bytes::BytesMut;
use pangya_protocol::{
    CheckNickname, CodecLimits, CompatibilityProfile, ConnectionState, DecodePacket, Direction,
    ErrorClass, FrameCodec, InboundFrame, LoginKey, LoginRequest, OutboundFrame, PacketReader,
    PacketRegistry, PacketWriter, RegistryKey, ServiceKind, SessionKey, UnknownBytes,
    us852_login_hello,
};
use proptest::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Decoder, Encoder};
use zeroize::Zeroizing;

fn encrypted_login() -> Vec<u8> {
    let mut writer = PacketWriter::new();
    writer.u16_le(LoginRequest::OPCODE);
    writer
        .pstring(b"synthetic-user", 64)
        .expect("fixture username fits");
    writer
        .pstring(b"00000000000000000000000000000000", 128)
        .expect("fixture secret fits");
    writer.bytes(&[0; 17]);
    pangya_crypto::client_encrypt(writer.as_slice(), 5, 0x42).expect("fixture frame fits")
}

#[test]
fn hello_matches_attributed_fixture_and_rejects_bad_key() {
    assert_eq!(
        us852_login_hello(5).expect("valid key"),
        *include_bytes!("fixtures/us852-login-hello/fixture.bin")
    );
    assert!(us852_login_hello(0x10).is_err());
}

#[test]
fn reader_writer_checked_surface_round_trips() {
    let mut writer = PacketWriter::new();
    writer.i8(-2);
    writer.u16_le(0x1234);
    writer.i32_le(-77);
    writer.f32_le(f32::from_bits(0x7fc0_1234));
    writer.pstring(b"raw", 8).expect("fits");
    writer.fixed_nul(b"x", 4).expect("fits");
    writer.count_u16(2, 4).expect("fits");
    let bytes = writer.into_inner();
    let mut reader = PacketReader::new(
        &bytes,
        Direction::ClientToServer,
        ServiceKind::Login,
        Some(1),
    );
    assert_eq!(reader.i8().expect("i8"), -2);
    assert_eq!(reader.u16_le().expect("u16"), 0x1234);
    assert_eq!(reader.i32_le().expect("i32"), -77);
    assert_eq!(reader.f32_le().expect("f32").to_bits(), 0x7fc0_1234);
    assert_eq!(reader.pstring(8).expect("string"), b"raw");
    assert_eq!(reader.fixed_nul(4).expect("fixed"), b"x");
    assert_eq!(reader.count_u16(4).expect("count"), 2);
    assert_eq!(reader.remaining(), 0);
    assert!(writer_overflow_examples());
}
fn writer_overflow_examples() -> bool {
    let mut writer = PacketWriter::new();
    writer.pstring(&[0; 5], 4).is_err()
        && writer.fixed_nul(b"abcd", 4).is_err()
        && writer.count_u16(5, 4).is_err()
}

#[test]
fn login_models_keep_unknown_bytes_explicit() {
    let plain = pangya_crypto::client_decrypt(&encrypted_login(), 5).expect("decrypt");
    let mut reader = PacketReader::new(
        &plain[2..],
        Direction::ClientToServer,
        ServiceKind::Login,
        Some(1),
    );
    let packet =
        LoginRequest::decode(&mut reader, &CompatibilityProfile::US_852).expect("typed login");
    assert_eq!(packet.username, b"synthetic-user");
    assert_eq!(packet.unknown_tail, vec![0; 17]);
    let mut nickname = PacketWriter::new();
    nickname.pstring(b"synthetic", 64).expect("fits");
    let mut reader = PacketReader::new(
        nickname.as_slice(),
        Direction::ClientToServer,
        ServiceKind::Login,
        Some(7),
    );
    assert_eq!(
        CheckNickname::decode(&mut reader, &CompatibilityProfile::US_852)
            .expect("nickname")
            .nickname,
        b"synthetic"
    );
}

#[test]
fn decoder_preserves_fragmented_and_coalesced_frames() {
    let frame = encrypted_login();
    let mut bytewise = BytesMut::new();
    let mut codec = FrameCodec::new(5, ServiceKind::Login, CodecLimits::default());
    for (index, byte) in frame.iter().enumerate() {
        bytewise.extend_from_slice(&[*byte]);
        let decoded = codec.decode(&mut bytewise).expect("bytewise prefix");
        if index + 1 == frame.len() {
            assert_eq!(decoded.expect("complete on final byte").opcode, 1);
        } else {
            assert!(decoded.is_none());
        }
    }
    for split in 0..=frame.len() {
        let mut codec = FrameCodec::new(5, ServiceKind::Login, CodecLimits::default());
        let mut src = BytesMut::new();
        src.extend_from_slice(&frame[..split]);
        let first = codec.decode(&mut src).expect("prefix is not malformed");
        if split < frame.len() {
            assert!(first.is_none());
            src.extend_from_slice(&frame[split..]);
        }
        let decoded = first
            .or_else(|| codec.decode(&mut src).expect("complete frame"))
            .expect("frame");
        assert_eq!(decoded.opcode, 1);
        assert!(src.is_empty());
    }
    let mut joined = BytesMut::new();
    joined.extend_from_slice(&frame);
    joined.extend_from_slice(&frame);
    joined.extend_from_slice(&frame[..3]);
    let mut codec = FrameCodec::new(5, ServiceKind::Login, CodecLimits::default());
    assert_eq!(
        codec
            .decode(&mut joined)
            .expect("first")
            .expect("first frame")
            .opcode,
        1
    );
    assert_eq!(
        codec
            .decode(&mut joined)
            .expect("second")
            .expect("second frame")
            .opcode,
        1
    );
    assert_eq!(joined.len(), 3);
    assert!(codec.decode(&mut joined).expect("partial").is_none());
}

#[test]
fn decoder_rejects_bad_lengths_and_eof_truncation_without_reserving() {
    let limits = CodecLimits {
        max_client_frame_bytes: 32,
        ..CodecLimits::default()
    };
    let mut codec = FrameCodec::new(5, ServiceKind::Login, limits);
    let mut oversized = BytesMut::from(&[0, 0xff, 0xff, 0][..]);
    let capacity = oversized.capacity();
    assert!(codec.decode(&mut oversized).is_err());
    assert_eq!(oversized.capacity(), capacity);
    assert_eq!(oversized.len(), 4);
    let frame = encrypted_login();
    for end in 1..frame.len() {
        let mut codec = FrameCodec::new(5, ServiceKind::Login, CodecLimits::default());
        let mut partial = BytesMut::from(&frame[..end]);
        assert!(codec.decode_eof(&mut partial).is_err());
    }
    let mut too_short = BytesMut::from(&[0, 0, 0, 0][..]);
    assert!(codec.decode(&mut too_short).is_err());

    for plain in [&[][..], &[0x01][..]] {
        let encrypted = pangya_crypto::client_encrypt(plain, 5, 7).expect("small frame");
        let mut src = BytesMut::from(encrypted.as_slice());
        let error = codec.decode(&mut src).expect_err("opcode is incomplete");
        assert!(matches!(
            error.context(),
            Some((
                Direction::ClientToServer,
                ServiceKind::Login,
                None,
                5,
                ErrorClass::Truncated
            ))
        ));
    }
}

#[test]
fn encoder_appends_and_output_decrypts() {
    let mut dst = BytesMut::from(&b"queued"[..]);
    let mut codec = FrameCodec::new(5, ServiceKind::Login, CodecLimits::default());
    codec
        .encode(
            OutboundFrame {
                opcode: 7,
                payload: Zeroizing::new(b"payload".to_vec()),
                salt: 9,
            },
            &mut dst,
        )
        .expect("encode");
    assert_eq!(&dst[..6], b"queued");
    let decoded =
        pangya_crypto::server_decrypt(&dst[6..], 5, 8 * 1024 * 1024, 128).expect("server frame");
    assert_eq!(&decoded[..2], &7_u16.to_le_bytes());
    assert_eq!(&decoded[2..], b"payload");
}

#[test]
fn all_keys_and_salts_decode_and_protocol_length_edges_are_explicit() {
    let plain = [1, 0, 0xaa];
    for key in 0..=15 {
        for salt in 0..=u8::MAX {
            let encrypted = pangya_crypto::client_encrypt(&plain, key, salt).expect("valid key");
            let mut src = BytesMut::from(encrypted.as_slice());
            let mut codec = FrameCodec::new(key, ServiceKind::Login, CodecLimits::default());
            assert_eq!(
                codec
                    .decode(&mut src)
                    .expect("decode")
                    .expect("frame")
                    .opcode,
                1
            );
        }
    }

    let operational_plain = vec![0; 65_530];
    let operational =
        pangya_crypto::client_encrypt(&operational_plain, 0, 0).expect("65,535 total");
    assert_eq!(operational.len(), 65_535);
    let mut src = BytesMut::from(operational.as_slice());
    assert!(
        FrameCodec::new(0, ServiceKind::Login, CodecLimits::default())
            .decode(&mut src)
            .expect("operational maximum")
            .is_some()
    );

    let theoretical_plain = vec![0; 65_534];
    let theoretical =
        pangya_crypto::client_encrypt(&theoretical_plain, 0, 0).expect("65,539 total");
    assert_eq!(theoretical.len(), 65_539);
    let mut rejected = BytesMut::from(theoretical.as_slice());
    assert!(
        FrameCodec::new(0, ServiceKind::Login, CodecLimits::default())
            .decode(&mut rejected)
            .is_err()
    );
    let mut accepted = BytesMut::from(theoretical.as_slice());
    let theoretical_limits = CodecLimits {
        max_client_frame_bytes: 65_539,
        ..CodecLimits::default()
    };
    assert!(
        FrameCodec::new(0, ServiceKind::Login, theoretical_limits)
            .decode(&mut accepted)
            .expect("protocol theoretical maximum")
            .is_some()
    );
    assert!(pangya_crypto::client_encrypt(&vec![0; 65_535], 0, 0).is_err());
}

#[test]
fn server_encoder_limits_overflow_and_append_failure_are_explicit() {
    let limits = CodecLimits {
        max_server_plaintext_bytes: usize::from(u16::MAX),
        ..CodecLimits::default()
    };
    let mut codec = FrameCodec::new(0, ServiceKind::Login, limits);
    let mut dst = BytesMut::from(&b"prefix"[..]);
    codec
        .encode(
            OutboundFrame {
                opcode: 1,
                payload: Zeroizing::new(vec![0; usize::from(u16::MAX) - 2]),
                salt: 0,
            },
            &mut dst,
        )
        .expect("exact 65,535-byte plaintext cap");
    let appended_len = dst.len();
    assert!(
        codec
            .encode(
                OutboundFrame {
                    opcode: 1,
                    payload: Zeroizing::new(vec![0; usize::from(u16::MAX) - 1]),
                    salt: 0,
                },
                &mut dst,
            )
            .is_err()
    );
    assert_eq!(dst.len(), appended_len);

    let mut incompressible = vec![0_u8; 65_535];
    let mut state = 0x1234_5678_u32;
    for byte in &mut incompressible {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *byte = (state >> 24) as u8;
    }
    assert!(matches!(
        pangya_crypto::server_encrypt(&incompressible, 0, 0, incompressible.len()),
        Err(pangya_crypto::CryptoError::LengthOverflow("server"))
    ));
}

#[test]
fn bounded_prefix_errors_report_the_prefix_start() {
    let bytes = [0xaa, 5, 0, 9, 0];
    let mut reader = PacketReader::new(
        &bytes,
        Direction::ClientToServer,
        ServiceKind::Login,
        Some(1),
    );
    assert_eq!(reader.u8().expect("prefix offset"), 0xaa);
    let error = reader.pstring(4).expect_err("over limit");
    assert!(matches!(
        error.context(),
        Some((
            Direction::ClientToServer,
            ServiceKind::Login,
            Some(1),
            1,
            ErrorClass::Limit
        ))
    ));
    let error = reader.count_u16(8).expect_err("over limit");
    assert!(matches!(
        error.context(),
        Some((_, _, Some(1), 3, ErrorClass::Limit))
    ));
}

#[test]
fn decoder_io_error_is_context_neutral() {
    let error = pangya_protocol::PacketDecodeError::from(std::io::Error::other("closed"));
    assert!(error.context().is_none());
    assert_eq!(error.to_string(), "transport I/O failed: closed");
}

#[test]
fn debug_output_redacts_secrets_and_raw_payloads() {
    let login = LoginRequest {
        username: b"user".to_vec(),
        password: b"password-secret".to_vec(),
        unknown_tail: b"tail-secret".to_vec(),
    };
    let session = SessionKey {
        unknown: UnknownBytes([0; 4]),
        session_key: b"session-secret".to_vec(),
    };
    let login_key = LoginKey {
        login_key: b"login-secret".to_vec(),
    };
    let inbound = InboundFrame {
        opcode: 1,
        payload: Zeroizing::new(b"inbound-secret".to_vec()),
        metadata: pangya_protocol::FrameMetadata {
            encrypted_len: 10,
            salt: 1,
        },
    };
    let outbound = OutboundFrame {
        opcode: 1,
        payload: Zeroizing::new(b"outbound-secret".to_vec()),
        salt: 1,
    };
    for (debug, secret) in [
        (format!("{login:?}"), "password-secret"),
        (format!("{session:?}"), "session-secret"),
        (format!("{login_key:?}"), "login-secret"),
        (format!("{inbound:?}"), "inbound-secret"),
        (format!("{outbound:?}"), "outbound-secret"),
    ] {
        assert!(!debug.contains(secret), "secret leaked by {debug}");
    }
    assert!(!format!("{login:?}").contains("tail-secret"));
}

#[test]
fn registry_is_state_aware() {
    let active = RegistryKey {
        service: ServiceKind::Login,
        direction: Direction::ClientToServer,
        version: CompatibilityProfile::US_852.version(),
        state: ConnectionState::AwaitingFirstPacket,
        opcode: 1,
    };
    let mut registry = PacketRegistry::new();
    assert!(registry.register(active));
    assert!(registry.accepts(active));
    assert!(!registry.accepts(RegistryKey {
        state: ConnectionState::Active,
        ..active
    }));
}

#[tokio::test]
async fn synthetic_tcp_hello_and_login_harness() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        socket
            .write_all(&us852_login_hello(5).expect("hello"))
            .await
            .expect("write hello");
        let mut bytes = BytesMut::with_capacity(256);
        loop {
            let read = socket.read_buf(&mut bytes).await.expect("read");
            assert!(read > 0);
            let mut codec = FrameCodec::new(5, ServiceKind::Login, CodecLimits::default());
            if let Some(frame) = codec.decode(&mut bytes).expect("decode") {
                assert_eq!(frame.opcode, 1);
                let mut reader = PacketReader::new(
                    &frame.payload,
                    Direction::ClientToServer,
                    ServiceKind::Login,
                    Some(frame.opcode),
                );
                return LoginRequest::decode(&mut reader, &CompatibilityProfile::US_852)
                    .expect("login")
                    .username
                    .clone();
            }
        }
    });
    let mut client = TcpStream::connect(address).await.expect("connect");
    let mut hello = [0; 14];
    client.read_exact(&mut hello).await.expect("read hello");
    assert_eq!(hello, us852_login_hello(5).expect("hello"));
    client
        .write_all(&encrypted_login())
        .await
        .expect("send login");
    assert_eq!(server.await.expect("join"), b"synthetic-user");
}

proptest! {
    #[test]
    fn arbitrary_reader_never_advances_past_input(data in prop::collection::vec(any::<u8>(),0..64)) {
        let mut reader=PacketReader::new(&data,Direction::ClientToServer,ServiceKind::Login,None); let _=reader.u64_le(); prop_assert!(reader.offset()<=data.len()); prop_assert_eq!(reader.offset()+reader.remaining(),data.len());
    }
}
