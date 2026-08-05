//! Generated M3 GameService fixture regression; no proprietary client bytes.

use pangya_protocol::synthetic_game_hello;

#[test]
fn synthetic_game_hello_matches_generated_golden() {
    let expected = include_bytes!("fixtures/game-out-hello-synthetic/packet.bin");
    assert_eq!(synthetic_game_hello(9).expect("hello").as_slice(), expected);
}
