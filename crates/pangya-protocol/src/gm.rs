//! Evidence-backed GameMaster command framing.
//!
//! The references agree on the opcode family and on the minimum command layouts.  The
//! remaining commands are deliberately represented as refused rather than being decoded with
//! guessed fields; this keeps an unknown GM request from becoming a state mutation.

/// Client opcode for the GM command multiplexer.
pub const GM_COMMAND: u16 = 0x008f;
/// Client opcode for the GM identity request/toggle.
pub const GM_IDENTITY: u16 = 0x0041;
/// Client opcode for a GM notice broadcast.
pub const GM_NOTICE: u16 = 0x0057;
/// Client opcode for destroying a room by number.
pub const GM_DESTROY_ROOM: u16 = 0x0060;
/// Client opcode for disconnecting a user by OID (legacy/stubbed in one reference).
pub const GM_DISCONNECT_USER: u16 = 0x0061;
/// Client opcode for entering/observing any room.
pub const GM_ENTER_ROOM: u16 = 0x003e;

/// Common GM command sub-command IDs.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GmSubcommand {
    /// Kick a player from their room (`OID`, force byte).
    Kick { oid: u32, force: u8 },
    /// Disconnect a player by OID.
    Disconnect { oid: u32 },
    /// Destroy a room by number.
    Destroy { room: u16 },
    /// Change versus wind (`speed`, `direction`).
    Wind { speed: u8, direction: u8 },
    /// Change weather.
    Weather { weather: u8 },
    /// Give a catalog item (`OID`, type ID, quantity).
    GiveItem {
        oid: u32,
        item_type_id: u32,
        quantity: u32,
    },
    /// A command whose layout is known by name but not sufficiently established by the local
    /// corpus. It must not be interpreted or applied.
    Refused { subcommand: u16 },
}

/// One accepted or safely refused GM request.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GmRequest {
    /// Multiplexed command.
    Command(GmSubcommand),
    /// Identity has a reference-backed body but is never allowed to elevate capability.
    Identity { capability: u32, nickname: Vec<u8> },
    /// Broadcast notice.
    Notice(Vec<u8>),
    /// Legacy command whose reference layout is unresolved.
    Refused { opcode: u16 },
}

/// Parsing/authorization failure. No error contains the request body.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GmRequestError {
    /// Body is not exactly the established wire shape.
    Malformed { opcode: u16 },
    /// A valid GM opcode was sent by an account without the persisted capability.
    Unauthorized { opcode: u16 },
}

/// Decode one GM request using only exact, reference-backed layouts.
///
/// Unresolved opcodes and subcommands return [`GmRequest::Refused`] and never expose their raw
/// bytes to callers.  The caller must log and ignore those requests, not route them through a
/// generic unknown-opcode disconnect path.
pub fn decode_gm_request(opcode: u16, payload: &[u8]) -> Result<GmRequest, GmRequestError> {
    match opcode {
        GM_COMMAND => {
            let Some((&lo, rest)) = payload.split_first() else {
                return Err(GmRequestError::Malformed { opcode });
            };
            let Some((&hi, body)) = rest.split_first() else {
                return Err(GmRequestError::Malformed { opcode });
            };
            let subcommand = u16::from_le_bytes([lo, hi]);
            let command = match subcommand {
                10 => {
                    if body.len() != 5 {
                        return Err(GmRequestError::Malformed { opcode });
                    }
                    GmSubcommand::Kick {
                        oid: u32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                        force: body[4],
                    }
                }
                11 => {
                    if body.len() != 4 {
                        return Err(GmRequestError::Malformed { opcode });
                    }
                    GmSubcommand::Disconnect {
                        oid: u32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                    }
                }
                13 => {
                    if body.len() != 2 {
                        return Err(GmRequestError::Malformed { opcode });
                    }
                    GmSubcommand::Destroy {
                        room: u16::from_le_bytes([body[0], body[1]]),
                    }
                }
                14 => {
                    if body.len() != 2 {
                        return Err(GmRequestError::Malformed { opcode });
                    }
                    GmSubcommand::Wind {
                        speed: body[0],
                        direction: body[1],
                    }
                }
                15 => {
                    if body.len() != 1 {
                        return Err(GmRequestError::Malformed { opcode });
                    }
                    GmSubcommand::Weather { weather: body[0] }
                }
                18 => {
                    if body.len() != 12 {
                        return Err(GmRequestError::Malformed { opcode });
                    }
                    GmSubcommand::GiveItem {
                        oid: u32::from_le_bytes([body[0], body[1], body[2], body[3]]),
                        item_type_id: u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
                        quantity: u32::from_le_bytes([body[8], body[9], body[10], body[11]]),
                    }
                }
                _ => GmSubcommand::Refused { subcommand },
            };
            Ok(GmRequest::Command(command))
        }
        GM_IDENTITY => {
            if payload.len() < 6 {
                return Err(GmRequestError::Malformed { opcode });
            }
            let capability = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
            let length = usize::from(u16::from_le_bytes([payload[4], payload[5]]));
            if length > 64 || payload.len() != 6 + length {
                return Err(GmRequestError::Malformed { opcode });
            }
            Ok(GmRequest::Identity {
                capability,
                nickname: payload[6..].to_vec(),
            })
        }
        GM_NOTICE => Ok(GmRequest::Notice(decode_pstring(opcode, payload, 256)?)),
        GM_DESTROY_ROOM => {
            if payload.len() != 2 {
                return Err(GmRequestError::Malformed { opcode });
            }
            Ok(GmRequest::Command(GmSubcommand::Destroy {
                room: u16::from_le_bytes([payload[0], payload[1]]),
            }))
        }
        GM_DISCONNECT_USER | GM_ENTER_ROOM => Ok(GmRequest::Refused { opcode }),
        _ => Err(GmRequestError::Malformed { opcode }),
    }
}

/// Enforce the server-side capability before interpreting any GM request.
pub fn authorize_gm_request(is_game_master: bool, opcode: u16) -> Result<(), GmRequestError> {
    if is_game_master {
        Ok(())
    } else {
        Err(GmRequestError::Unauthorized { opcode })
    }
}

fn decode_pstring(opcode: u16, payload: &[u8], maximum: usize) -> Result<Vec<u8>, GmRequestError> {
    if payload.len() < 2 {
        return Err(GmRequestError::Malformed { opcode });
    }
    let length = usize::from(u16::from_le_bytes([payload[0], payload[1]]));
    if length > maximum || payload.len() != length + 2 {
        return Err(GmRequestError::Malformed { opcode });
    }
    Ok(payload[2..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_fixtures_decode_minimum_command_union() {
        assert_eq!(
            decode_gm_request(GM_COMMAND, &[10, 0, 0x2a, 0, 0, 0, 1]),
            Ok(GmRequest::Command(GmSubcommand::Kick { oid: 42, force: 1 }))
        );
        assert_eq!(
            decode_gm_request(GM_COMMAND, &[14, 0, 7, 180]),
            Ok(GmRequest::Command(GmSubcommand::Wind {
                speed: 7,
                direction: 180
            }))
        );
        assert_eq!(
            decode_gm_request(
                GM_COMMAND,
                &[18, 0, 0x2a, 0, 0, 0, 0xc9, 0, 0, 0, 3, 0, 0, 0]
            ),
            Ok(GmRequest::Command(GmSubcommand::GiveItem {
                oid: 42,
                item_type_id: 201,
                quantity: 3,
            }))
        );
    }

    #[test]
    fn unresolved_union_members_are_refused_without_body_interpretation() {
        assert_eq!(
            decode_gm_request(GM_COMMAND, &[16, 0, 0xff, 0xff]),
            Ok(GmRequest::Command(GmSubcommand::Refused { subcommand: 16 }))
        );
        assert_eq!(
            decode_gm_request(GM_ENTER_ROOM, &[0xff; 32]),
            Ok(GmRequest::Refused {
                opcode: GM_ENTER_ROOM
            })
        );
        assert_eq!(
            decode_gm_request(GM_DISCONNECT_USER, &[0xff; 32]),
            Ok(GmRequest::Refused {
                opcode: GM_DISCONNECT_USER
            })
        );
    }

    #[test]
    fn unauthorized_is_explicit_and_cannot_mutate_a_request() {
        let request = decode_gm_request(GM_COMMAND, &[15, 0, 2]).expect("fixture");
        assert_eq!(
            authorize_gm_request(false, GM_COMMAND),
            Err(GmRequestError::Unauthorized { opcode: GM_COMMAND })
        );
        assert_eq!(
            request,
            GmRequest::Command(GmSubcommand::Weather { weather: 2 })
        );
    }
}
