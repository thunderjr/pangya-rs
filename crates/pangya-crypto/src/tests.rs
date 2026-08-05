use super::*;
use proptest::prelude::*;
use sha2::{Digest, Sha256};

const CLIENT_VECTORS: [(&[u8], &[u8], u8); 3] = [
    (
        include_bytes!("../tests/fixtures/client-1/fixture.bin"),
        include_bytes!("../tests/fixtures/client-1/expected.bin"),
        0,
    ),
    (
        include_bytes!("../tests/fixtures/client-2/fixture.bin"),
        include_bytes!("../tests/fixtures/client-2/expected.bin"),
        5,
    ),
    (
        include_bytes!("../tests/fixtures/client-3/fixture.bin"),
        include_bytes!("../tests/fixtures/client-3/expected.bin"),
        5,
    ),
];
const SERVER_VECTORS: [(&[u8], &[u8], u8); 3] = [
    (
        include_bytes!("../tests/fixtures/server-1/fixture.bin"),
        include_bytes!("../tests/fixtures/server-1/expected.bin"),
        0,
    ),
    (
        include_bytes!("../tests/fixtures/server-2/fixture.bin"),
        include_bytes!("../tests/fixtures/server-2/expected.bin"),
        7,
    ),
    (
        include_bytes!("../tests/fixtures/server-3/fixture.bin"),
        include_bytes!("../tests/fixtures/server-3/expected.bin"),
        5,
    ),
];

#[test]
fn oracle_integrity_hashes_match_attributed_source() {
    let table_zero = Sha256::digest(ORACLE[0]);
    let table_one = Sha256::digest(ORACLE[1]);
    let combined = Sha256::digest(ORACLE.concat());
    assert_eq!(
        format!("{table_zero:x}"),
        "6eee0700c7096c57a992b1cf787e06d2b661cfb0fd8871c481e089c3a55fabfe"
    );
    assert_eq!(
        format!("{table_one:x}"),
        "89a66b67ca44457bc782c85cffff41abb45930c723983fd03bd9f2bc20b331d7"
    );
    assert_eq!(
        format!("{combined:x}"),
        "003d0b42f9fc1e2fb3b9dc37d23bbe1ff018669ea81bf0068abfaea4942b7133"
    );
}

#[test]
fn all_three_client_vectors_match() {
    for (cipher, plain, key) in CLIENT_VECTORS {
        assert_eq!(
            client_decrypt(cipher, key).expect("decrypt").as_slice(),
            plain
        );
        assert_eq!(client_encrypt(plain, key, cipher[0]), Ok(cipher.to_vec()));
    }
}

#[test]
fn representative_server_vectors_decompress() {
    for (cipher, plain, key) in SERVER_VECTORS {
        assert_eq!(
            server_decrypt(cipher, key, 8 * 1024 * 1024, 128)
                .expect("decrypt")
                .as_slice(),
            plain
        );
        let generated = server_encrypt(plain, key, cipher[0], 8 * 1024 * 1024)
            .expect("test vector must compress");
        assert_eq!(
            server_decrypt(&generated, key, 8 * 1024 * 1024, 128)
                .expect("decrypt")
                .as_slice(),
            plain
        );
    }
}

#[test]
fn rejects_invalid_keys_headers_lengths_and_truncation() {
    assert!(matches!(
        client_encrypt(&[], 0x10, 0),
        Err(CryptoError::InvalidKey(0x10))
    ));
    assert!(matches!(
        server_encrypt(&[], 0x10, 0, 1),
        Err(CryptoError::InvalidKey(0x10))
    ));
    for plain in [&[][..], &[0][..]] {
        assert!(matches!(
            server_encrypt(plain, 0, 0, 2),
            Err(CryptoError::PlaintextTooShort { .. })
        ));
    }
    assert!(matches!(
        client_decrypt(&[0; 4], 0),
        Err(CryptoError::HeaderTooShort { .. })
    ));
    assert!(matches!(
        server_decrypt(&[0; 7], 0, 100, 10),
        Err(CryptoError::HeaderTooShort { .. })
    ));
    let truncated = &SERVER_VECTORS[0].0[..SERVER_VECTORS[0].0.len() - 1];
    assert!(matches!(
        server_decrypt(truncated, 0, 100, 128),
        Err(CryptoError::LengthMismatch { .. })
    ));

    // The outer frame is internally consistent; its one-byte compressed body is not LZO1X.
    let corrupt_lzo = [0, 6, 0, 0, 0, 0, 0, ORACLE[1][0], 0];
    assert!(matches!(
        server_decrypt(&corrupt_lzo, 0, 128, 128),
        Err(CryptoError::Lzo(_))
    ));

    // Valid LZO streams that do not contain a complete opcode are still invalid server packets.
    let valid_empty_lzo = [0, 8, 0, 0, 0, 0, 0, ORACLE[1][0], 0x11, 0, 0];
    assert!(matches!(
        server_decrypt(&valid_empty_lzo, 0, 128, 128),
        Err(CryptoError::PlaintextTooShort {
            actual: 0,
            minimum: 2
        })
    ));

    let compressed_one = lzokay::compress::compress(&[0x7f]).expect("one byte compresses");
    let mut valid_one_lzo = vec![0; compressed_one.len() + SERVER_HEADER];
    valid_one_lzo[SERVER_HEADER..].copy_from_slice(&compressed_one);
    let encoded_length = u16::try_from(valid_one_lzo.len() - 3).expect("small fixture");
    valid_one_lzo[1..3].copy_from_slice(&encoded_length.to_le_bytes());
    valid_one_lzo[7] = 1 ^ ORACLE[1][0];
    for offset in (10..valid_one_lzo.len()).rev() {
        valid_one_lzo[offset] ^= valid_one_lzo[offset - 4];
    }
    assert!(matches!(
        server_decrypt(&valid_one_lzo, 0, 128, 128),
        Err(CryptoError::PlaintextTooShort {
            actual: 1,
            minimum: 2
        })
    ));
}

#[test]
fn decompression_caps_are_strict() {
    let (cipher, _, key) = SERVER_VECTORS[2];
    assert!(server_decrypt(cipher, key, 16, 128).is_err());
    assert!(server_decrypt(cipher, key, 8 * 1024 * 1024, 1).is_err());
}

proptest! {
    #[test]
    fn client_round_trip(data in prop::collection::vec(any::<u8>(), 0..4096), key in 0_u8..=15, salt: u8) {
        let encrypted = client_encrypt(&data, key, salt)?;
        let decrypted = client_decrypt(&encrypted, key)?;
        prop_assert_eq!(decrypted.as_slice(), data.as_slice());
    }

    #[test]
    fn server_lzokay_round_trip(data in prop::collection::vec(any::<u8>(), 2..8192), key in 0_u8..=15, salt: u8) {
        let encrypted = server_encrypt(&data, key, salt, 8192)?;
        let decrypted = server_decrypt(&encrypted, key, 8192, 128)?;
        prop_assert_eq!(decrypted.as_slice(), data.as_slice());
    }
}
