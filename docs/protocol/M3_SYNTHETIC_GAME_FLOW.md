# M3 local synthetic GameService flow

This flow is a generated local contract, not a claim of U.S. 852 acceptance.

1. GameService sends a four-byte plaintext hello. The negotiated transport key
   is the final byte; the three leading bytes are synthetic.
2. Client sends encrypted opcode `0x0002`: `claimed_account_id:u64` and a bounded
   `u16`-length bearer. The bearer is redacted and zeroized.
3. Server parses and atomically consumes the M2 handover for target Game. The
   consumed account ID is authoritative; the claimed ID is untrusted and must
   match or the connection closes.
4. Server acquires duplicate-account presence, loads one repeatable-read
   PlayerSnapshot, requires active/complete/owned coherent state, and validates
   Character/ClubSet/Ball type IDs against the immutable catalog.
5. Server emits, in local order:
   - `0x0070`: account ID, nickname, Pang, points, experience.
   - `0x0072`: bounded `(character_id,type_id)` records.
   - `0x0073`: inventory segments of at most 50
     `(inventory_id,type_id,quantity)` records.
   - `0x004d`: selected character, equipped club/ball, version.
6. Client sends `0x0004 channel_id:u32`. Only the configured ID is accepted.
7. Server sends `0x004e channel_id:u32` and retains presence in `InChannel` until
   EOF, cancellation, timeout, or protocol rejection.

All packet bodies beyond these fields are intentionally absent, not guessed.
Malformed, unknown, or state-invalid opcodes close immediately because encrypted
transport cannot safely resynchronize. Global/source pre-spawn admission,
accept/auth/packet/byte rates, total authentication and idle deadlines, bounded
drain, and RAII presence apply throughout.

External gates: actual hello length/content, opcode layouts and ordering, channel
semantics, player bootstrap fields, IFF record layouts, and acceptance by a
legally held U.S. 852 client/capture.
