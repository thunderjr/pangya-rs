# U.S. 852 MessageService

MessageService is a separate service and opcode namespace. Its client IDs `0x12`–`0x2d`
collide with GameService IDs and must not be dispatched through the game registry.

The implementation follows the local packetdoc corpus:

- `0x12` credential declaration (`u32` account ID plus PString nickname)
- `0x14` hello and `0x16` goodbye
- `0x17` lookup and `0x18` friend request
- `0x1d` status declaration and `0x1e` friend chat
- `0x23` game server/channel declaration
- server `0x2f` credential acknowledgement and `0x30` status packets

Friend state is directed (alias, block, and pending flags), while confirmation and deletion
apply both directions. Offline messages are bounded to 512 bytes and are removed atomically when
claimed. The PostgreSQL migration owns the durable rows; the process-local store is used by
isolated protocol tests and can be shared between listener generations.

LoginService `0x0009` and GameService `0x00fc` advertise the same endpoint record. A game
`0x008b` request receives the `0x00fc` response rather than being silently ignored.

## Evidence

Layouts are derived from `opensource-references/pangbox--packetdoc/src/packets/messageservice/`
and endpoint records from `src/packets/common/message_server.ksy`; social mutation and broadcast
semantics were cross-checked against SuperSS `Server Lib/Message Server` handlers.
