#!/usr/bin/env python3
"""Author deterministic PangYa shop data from a legally supplied client IFF.

``data/pangya_gb.iff`` is a ZIP inside the client's PAK series. Its item tables contain an
eight-byte ``<count,binding,version>`` header and fixed-width records. This tool keeps every
opaque client field intact, disables every existing shop offer in explicitly managed tables,
then enables and reprices only the type IDs declared by an operator-owned JSON catalog.

A one-entry PAK is useful for format diagnostics, but the U.S. 852 client does not reliably
replace an earlier duplicate with a later PAK. ``--replace-in-pak`` therefore rebuilds the exact
PAK that already wins for the client: it preserves the original body and file table, appends the
authored IFF before a relocated table, and rewrites only that entry's encrypted metadata. The
result is the simple manual-sync deployment unit until a dedicated updater exists.

No proprietary bytes are built into this program. The base archive and generated outputs must
remain outside Git (normally under ``local-data/``).

Catalog example::

    {
      "version": 1,
      "managed_tables": ["ClubSet.iff", "Ball.iff"],
      "offers": [
        {"table": "ClubSet.iff", "type_id": "0x10000000", "pang": 2500},
        {"table": "Ball.iff", "type_id": "0x14000000", "pang": 25}
      ]
    }

The selected row must already be a real client shop row. Its original non-zero shop flag is
preserved; this tool deliberately cannot invent unknown client metadata for an unsold row.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import struct
import tempfile
import zipfile
from dataclasses import dataclass

HEADER_BYTES = 8
TYPE_ID_OFFSET = 0x04
PRICE_OFFSET = 0x5C
SHOP_FLAG_OFFSET = 0x68
# The currency lives in its own byte, not in the low nibble of the one above.
# `opensource-references/pangbox--server/pangya/iff/item.go` lays the header out as
# `0x68 ShopFlag`, `0x69 MoneyFlag`, `0x6A TimeFlag`, `0x6B TimeByte`.
MONEY_FLAG_OFFSET = 0x69
MIN_PRICED_RECORD_BYTES = MONEY_FLAG_OFFSET + 1
PAK_SIGNATURE = 0x12
PAK_TRAILER_BYTES = 9
PAK_ENTRY_TYPE_MASK = 0xE0
PAK_ENTRY_XOR = 0x00
PAK_ENTRY_XTEA = 0x20
PAK_ENTRY_BASIC = 0x80
PAK_ENTRY_XTEA_LZ2 = 0x23
XTEA_ROUNDS = 16
XTEA_DELTA = 0x61C88647
MASK32 = 0xFFFF_FFFF
REGION_KEYS = {
    "us": (0x03F607A9, 0x036F5A3E, 0x011002B4, 0x04AB00EA),
    "jp": (0x020A5FD4, 0x01EEBDFF, 0x02B3C6A0, 0x04F6A3E1),
    "th": (0x050AD33B, 0x00BAFF09, 0x0452FFDA, 0x02CB4422),
    "eu": (0x01E986D8, 0x05818479, 0x03D2B0BB, 0x02C9B030),
    "id": (0x01640DB7, 0x01455A9B, 0x027F1AB7, 0x05918B54),
    "kr": (0x0485B576, 0x05148E02, 0x05141D96, 0x028FA9D6),
}
CLIENT_UNAVAILABLE_PRICE = 10_000_000
MAX_PANG_PRICE = 0xFFFF_FFFE
# The bit that decides whether the client's shop UI lists a row at all.
#
# Measured across the pristine U.S. 851 tables: a row with this bit set is *never* priced at the
# 10,000,000 unavailable sentinel, and a row without it almost always is — Part.iff 2,036 listed
# rows / 0 sentinels versus 5,289 unlisted / 4,365 sentinels, and the same split holds in Ball,
# ClubSet and Item. Enabling a row therefore means setting this bit; a priced row without it is
# purchasable by the protocol and invisible in the client, which is exactly the state SSAF,
# wings and rings were left in.
SHOP_LISTED_BIT = 0x20
# `MoneyFlag` values seen on listed rows are 0, 1 and 2. Authoring a Pang shop writes 0, which is
# what the overwhelming majority of listed Pang rows already carry, so a row cannot be listed at
# a Pang price the client then tries to charge in another currency.
MONEY_FLAG_PANG = 0x00
SERVER_TABLE_KINDS = {
    "Character.iff": "character",
    "ClubSet.iff": "club_set",
    "Ball.iff": "ball",
    "Item.iff": "consumable",
    "Part.iff": "character_part",
    "Course.iff": "course",
    # The client's own shop has six tabs; the six tables above cover three of them. These eight
    # are what Caddie, Mascot, Decoration and Card Holic are drawn from — always present in the
    # client, never parsed by the server, so every purchase from those tabs was refused with
    # `not_in_catalog`.
    #
    # `AddonPart.iff` is excluded on purpose: its type-id tags are 0x04 and 0x08, the same space
    # Character and CharacterPart occupy, so admitting it would make a type id ambiguous across
    # families for the sake of three shop rows.
    "Caddie.iff": "caddie",
    "CaddieItem.iff": "caddie_item",
    "Mascot.iff": "mascot",
    "Card.iff": "card",
    "Furniture.iff": "furniture",
    "Skin.iff": "skin",
    "HairStyle.iff": "hair_style",
    "SetItem.iff": "set_item",
}


class AuthorError(ValueError):
    """A catalog or source archive is not safe to author."""


@dataclass(frozen=True)
class Offer:
    table: str
    type_id: int
    pang: int


def parse_type_id(value: object) -> int:
    if isinstance(value, bool):
        raise AuthorError("type_id must be an integer or 0x-prefixed string")
    if isinstance(value, int):
        parsed = value
    elif isinstance(value, str):
        try:
            parsed = int(value, 0)
        except ValueError as error:
            raise AuthorError(f"invalid type_id {value!r}") from error
    else:
        raise AuthorError("type_id must be an integer or 0x-prefixed string")
    if not 0 < parsed <= 0xFFFF_FFFF:
        raise AuthorError(f"type_id is outside u32: {parsed}")
    return parsed


def load_catalog(path: pathlib.Path) -> tuple[list[str], list[Offer], bool]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AuthorError(f"cannot read catalog {path}: {error}") from error
    if not isinstance(value, dict) or value.get("version") != 1:
        raise AuthorError("catalog must be an object with version 1")

    managed = value.get("managed_tables")
    if not isinstance(managed, list) or not managed:
        raise AuthorError("managed_tables must be a non-empty array")
    if any(not isinstance(name, str) or not name.endswith(".iff") for name in managed):
        raise AuthorError("every managed table must be an .iff filename")
    if len(set(managed)) != len(managed):
        raise AuthorError("managed_tables contains a duplicate")

    raw_offers = value.get("offers")
    if not isinstance(raw_offers, list) or not raw_offers:
        raise AuthorError("offers must be a non-empty array")
    offers: list[Offer] = []
    seen: set[tuple[str, int]] = set()
    for index, raw in enumerate(raw_offers):
        if not isinstance(raw, dict):
            raise AuthorError(f"offer {index} must be an object")
        table = raw.get("table")
        if table not in managed:
            raise AuthorError(f"offer {index} names unmanaged table {table!r}")
        type_id = parse_type_id(raw.get("type_id"))
        pang = raw.get("pang")
        if isinstance(pang, bool) or not isinstance(pang, int) or not 0 < pang <= MAX_PANG_PRICE:
            raise AuthorError(f"offer {index} has an invalid Pang price")
        key = (table, type_id)
        if key in seen:
            raise AuthorError(f"duplicate offer {table}:{type_id:#010x}")
        seen.add(key)
        offers.append(Offer(table, type_id, pang))

    # Opt-in, and deliberately not the default. Without it this tool refuses to author a row the
    # client never listed, because it would be inventing metadata no evidence covers. With it,
    # such a row is given shop flag 0x02 — "Pang, tradeable", which 936 rows of the pristine
    # U.S. tables carry verbatim, so it is a byte the retail client is known to render.
    #
    # What it cannot promise: that a row the client never intended to sell *looks* right in the
    # shop. Quest and reward rows may have no icon, no description, or a minimum level that hides
    # them. Enabling everything is an operator's decision to accept that.
    invent = value.get("invent_shop_metadata", False)
    if not isinstance(invent, bool):
        raise AuthorError("invent_shop_metadata must be a boolean")
    return managed, offers, invent


def table_shape(data: bytes, table: str) -> tuple[int, int]:
    if len(data) < HEADER_BYTES:
        raise AuthorError(f"{table} is shorter than its header")
    count = struct.unpack_from("<H", data, 0)[0]
    if count == 0 or (len(data) - HEADER_BYTES) % count != 0:
        raise AuthorError(f"{table} has an invalid record count/width")
    record_size = (len(data) - HEADER_BYTES) // count
    if record_size < MIN_PRICED_RECORD_BYTES:
        raise AuthorError(f"{table} records are too short to carry retail prices")
    return count, record_size


def pang_shop_flag(original: int, table: str, type_id: int, invent: bool = False) -> int:
    """Returns the ``ShopFlag`` byte that makes a row appear in the client's shop.

    This replaces an earlier model that read the low nibble of ``0x68`` as a currency selector
    (``1`` Points, ``0``/``2`` Pang forms) and the upper nibble as display state. That model was
    wrong in a way that produced a working server and an empty-looking shop:
    ``pangbox--server/pangya/iff/item.go`` lays the header out as ``0x68 ShopFlag`` followed by
    a **separate** ``0x69 MoneyFlag``, so the currency was never in this byte at all. Under the
    old model 6,624 of 9,235 authored rows — SSAF, wings, rings, most of Part.iff — were given
    flags with :data:`SHOP_LISTED_BIT` clear. The server sold them happily and the client never
    drew one.

    The correct operation is to **set the listed bit and preserve everything else**. The other
    bits are independent of currency — Part.iff carries ``0x21`` against MoneyFlag 0, 1 and 2
    alike — so they encode something this project has not identified and has no reason to
    disturb. ``0x20`` on its own is attested: 104 pristine Skin rows and 9 Part rows carry
    exactly that, and ``0x21``/``0x22``/``0x60`` all survive untouched, which keeps the
    conversion proven in ``docs/evidence/REAL_CLIENT_SHOP_2026-08-09.md`` intact.
    """
    # A row the client never listed has no metadata to preserve — possibly no icon and no
    # description either. Listing one anyway is the operator's call, never a silent default.
    if original == 0:
        if invent:
            return SHOP_LISTED_BIT
        raise AuthorError(
            f"{table} type {type_id:#010x} was not a client shop row; "
            "refusing to invent its metadata"
        )
    return original | SHOP_LISTED_BIT


def author_table(
    data: bytes, table: str, offers: list[Offer], invent: bool = False
) -> tuple[bytes, list[dict[str, object]]]:
    count, record_size = table_shape(data, table)
    output = bytearray(data)
    rows: dict[int, tuple[int, int]] = {}
    for index in range(count):
        start = HEADER_BYTES + index * record_size
        active = struct.unpack_from("<I", data, start)[0]
        type_id = struct.unpack_from("<I", data, start + TYPE_ID_OFFSET)[0]
        if active != 0 and type_id != 0:
            rows[type_id] = (start, data[start + SHOP_FLAG_OFFSET])
            # The retail rows mark unavailable items redundantly: zero shop flags *and* the
            # 10,000,000 sentinel price. Preserve that exact convention so both the server
            # parser and the proprietary client agree that an unlisted row is not an offer.
            struct.pack_into("<I", output, start + PRICE_OFFSET, CLIENT_UNAVAILABLE_PRICE)
            output[start + SHOP_FLAG_OFFSET] = 0

    report: list[dict[str, object]] = []
    for offer in offers:
        row = rows.get(offer.type_id)
        if row is None:
            raise AuthorError(f"{table} does not contain active type {offer.type_id:#010x}")
        start, original_flag = row
        # The never-a-shop-row refusal now lives in `pang_shop_flag`, so both it and the unknown
        # currency case are governed by the same opt-in rather than by two separate rules.
        authored_flag = pang_shop_flag(original_flag, table, offer.type_id, invent)
        # Without the listed bit the client draws nothing while the server sells the row and this
        # report calls it an offer — a shop that is simultaneously full and empty depending on
        # which side you ask. That is precisely how 6,624 rows were lost under the previous
        # model, so it is a refusal rather than a warning.
        if not authored_flag & SHOP_LISTED_BIT:
            raise AuthorError(
                f"{table} type {offer.type_id:#010x} would author to an unlisted shop flag "
                f"{authored_flag:#04x} (original {original_flag:#04x}); "
                "refusing to report it as an offer"
            )
        struct.pack_into("<I", output, start + PRICE_OFFSET, offer.pang)
        output[start + SHOP_FLAG_OFFSET] = authored_flag
        # The price written above is in Pang, so the currency byte has to say so. Leaving a
        # row's retail MoneyFlag in place would list a 1-Pang item the client tries to charge as
        # Points — 329 Part.iff rows and 15 Item.iff rows are shaped that way.
        original_money = data[start + MONEY_FLAG_OFFSET]
        output[start + MONEY_FLAG_OFFSET] = MONEY_FLAG_PANG
        report.append(
            {
                "table": table,
                "type_id": f"0x{offer.type_id:08x}",
                "pang": offer.pang,
                "original_shop_flag": original_flag,
                "shop_flag": authored_flag,
                "original_money_flag": original_money,
                "money_flag": MONEY_FLAG_PANG,
            }
        )
    return bytes(output), report


def atomic_write(path: pathlib.Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def author_archive(  # noqa: PLR0913 - one more flag than ideal; splitting it hides the pairing
    source: pathlib.Path,
    destination: pathlib.Path,
    managed: list[str],
    offers: list[Offer],
    invent: bool = False,
) -> list[dict[str, object]]:
    per_table = {table: [offer for offer in offers if offer.table == table] for table in managed}
    if any(not rows for rows in per_table.values()):
        empty = [table for table, rows in per_table.items() if not rows]
        raise AuthorError(f"managed table has no offers: {', '.join(empty)}")

    try:
        with zipfile.ZipFile(source, "r") as archive:
            infos = archive.infolist()
            names = [info.filename for info in infos]
            missing = sorted(set(managed) - set(names))
            if missing:
                raise AuthorError(f"base archive is missing: {', '.join(missing)}")
            members = [(info, archive.read(info)) for info in infos]
    except (OSError, zipfile.BadZipFile, RuntimeError) as error:
        raise AuthorError(f"cannot read base archive {source}: {error}") from error

    report: list[dict[str, object]] = []
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{destination.name}.", dir=destination.parent)
    os.close(fd)
    try:
        with zipfile.ZipFile(temporary, "w") as output:
            for info, data in members:
                if info.filename in per_table:
                    data, authored = author_table(
                        data, info.filename, per_table[info.filename], invent
                    )
                    report.extend(authored)
                output.writestr(info, data)
        os.replace(temporary, destination)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise
    return report


def write_server_iff_directory(archive_path: pathlib.Path, destination: pathlib.Path) -> dict[str, object]:
    """Writes the server catalog from the exact authored archive used in the client PAK.

    Files are written before ``manifest.toml`` so a failed run cannot leave a new manifest that
    attests incomplete payloads. The server performs its own hash/header/record validation at
    startup; this function makes client/server drift impossible within one successful authoring
    run.
    """
    try:
        with zipfile.ZipFile(archive_path, "r") as archive:
            payloads = {name: archive.read(name) for name in SERVER_TABLE_KINDS}
    except (OSError, KeyError, zipfile.BadZipFile, RuntimeError) as error:
        raise AuthorError(f"cannot build server IFF directory from {archive_path}: {error}") from error

    destination.mkdir(parents=True, exist_ok=True)
    manifest = ["manifest_version = 3", ""]
    files: list[dict[str, object]] = []
    for filename, kind in SERVER_TABLE_KINDS.items():
        payload = payloads[filename]
        count, record_size = table_shape(payload, filename)
        binding, version = struct.unpack_from("<HI", payload, 2)
        digest = hashlib.sha256(payload).hexdigest()
        atomic_write(destination / filename, payload)
        manifest.extend(
            [
                "[[files]]",
                f'filename = "{filename}"',
                f'sha256 = "{digest}"',
                f'kind = "{kind}"',
                f"count = {count}",
                f"binding = {binding}",
                f"version = {version}",
                f"record_size = {record_size}",
                "",
            ]
        )
        files.append(
            {
                "filename": filename,
                "sha256": digest,
                "kind": kind,
                "count": count,
                "binding": binding,
                "version": version,
                "record_size": record_size,
            }
        )
    manifest_bytes = ("\n".join(manifest).rstrip() + "\n").encode("utf-8")
    atomic_write(destination / "manifest.toml", manifest_bytes)
    return {
        "directory": str(destination),
        "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
        "files": files,
    }


def encode_pak_path(entry_name: str) -> bytes:
    try:
        path = entry_name.replace("\\", "/").encode("euc-kr")
    except UnicodeEncodeError as error:
        raise AuthorError("PAK entry name is not representable in EUC-KR") from error
    if not path or len(path) > 255 or entry_name.startswith(("/", "\\")) or ".." in entry_name.split("/"):
        raise AuthorError("PAK entry name must be a relative path of at most 255 bytes")
    return path


def build_basic_pak(entry_name: str, data: bytes) -> bytes:
    """Builds the reference reader's unencrypted debugging form.

    Retail output uses :func:`build_retail_pak`; keeping this form makes the trailing-byte rule
    testable and gives operators a format diagnostic without claiming client compatibility.
    """
    path = encode_pak_path(entry_name)
    size = len(data)
    if size > 0xFFFF_FFFF:
        raise AuthorError("authored IFF is too large for the PAK format")
    # The basic entry form still carries one trailing path byte. The reference reader consumes
    # PathLength + 1 and exposes only PathLength bytes (pak/reader.go, EntryTypeBasic branch).
    entry = struct.pack("<BBIII", len(path), PAK_ENTRY_BASIC, 0, size, size) + path + b"\0"
    trailer = struct.pack("<IIB", size, 1, PAK_SIGNATURE)
    return data + entry + trailer


def encrypt_block(key: tuple[int, int, int, int], block: bytes) -> bytes:
    """Encrypts one block with PangYa's sixteen-round XTEA variant."""
    data0, data1 = struct.unpack("<II", block)
    total = 0
    for _ in range(XTEA_ROUNDS):
        mixed = ((((data1 << 4) & MASK32) ^ (data1 >> 5)) + data1) & MASK32
        data0 = (data0 + (mixed ^ ((total + key[total & 3]) & MASK32))) & MASK32
        total = (total - XTEA_DELTA) & MASK32
        mixed = ((((data0 << 4) & MASK32) ^ (data0 >> 5)) + data0) & MASK32
        data1 = (data1 + (mixed ^ ((total + key[(total >> 11) & 3]) & MASK32))) & MASK32
    return struct.pack("<II", data0, data1)


def encipher(key: tuple[int, int, int, int], data: bytes) -> bytes:
    if len(data) % 8 != 0:
        raise AuthorError("XTEA input must be a whole number of blocks")
    return b"".join(encrypt_block(key, data[offset : offset + 8]) for offset in range(0, len(data), 8))


def decrypt_block(key: tuple[int, int, int, int], block: bytes) -> bytes:
    """Decrypts one block with the same sixteen-round variant."""
    data0, data1 = struct.unpack("<II", block)
    total = (-XTEA_DELTA * XTEA_ROUNDS) & MASK32
    for _ in range(XTEA_ROUNDS):
        mixed = ((((data0 << 4) & MASK32) ^ (data0 >> 5)) + data0) & MASK32
        data1 = (data1 - (mixed ^ ((total + key[(total >> 11) & 3]) & MASK32))) & MASK32
        total = (total + XTEA_DELTA) & MASK32
        mixed = ((((data1 << 4) & MASK32) ^ (data1 >> 5)) + data1) & MASK32
        data0 = (data0 - (mixed ^ ((total + key[total & 3]) & MASK32))) & MASK32
    return struct.pack("<II", data0, data1)


def decipher(key: tuple[int, int, int, int], data: bytes) -> bytes:
    if len(data) % 8 != 0:
        raise AuthorError("XTEA input must be a whole number of blocks")
    return b"".join(decrypt_block(key, data[offset : offset + 8]) for offset in range(0, len(data), 8))


def literal_lz2(data: bytes) -> bytes:
    """Encodes valid LZ2 using literal-only groups.

    The untouched sequence byte must decode to zero, so each group starts with ``0xc8``. This is
    intentionally simple: an authored overlay is small and correctness matters more than ratio.
    """
    output = bytearray()
    for offset in range(0, len(data), 8):
        output.append(0xC8)
        output.extend(data[offset : offset + 8])
    return bytes(output)


def build_retail_pak(entry_name: str, data: bytes, region: str) -> bytes:
    """Builds the modern XTEA/LZ2 form used by the acquired U.S. PAK chain."""
    path = encode_pak_path(entry_name)
    key = REGION_KEYS[region]
    padded_length = (len(path) + 7) & ~7
    padded_path = path + b"\xcd" * (padded_length - len(path))
    encrypted_path = encipher(key, padded_path)
    packed = literal_lz2(data)
    if max(len(data), len(packed)) > 0xFFFF_FFFF:
        raise AuthorError("authored IFF is too large for the PAK format")

    raw = bytearray(
        struct.pack(
            "<BBIII",
            len(encrypted_path),
            PAK_ENTRY_XTEA_LZ2,
            0,
            len(packed),
            len(data),
        )
    )
    metadata = encipher(key, bytes(raw[2:6] + raw[10:14]))
    raw[2:6] = metadata[0:4]
    raw[10:14] = metadata[4:8]
    entry = bytes(raw) + encrypted_path
    trailer = struct.pack("<IIB", len(packed), 1, PAK_SIGNATURE)
    return packed + entry + trailer


def _read_pak_table(
    stream, list_offset: int, file_count: int, key: tuple[int, int, int, int]
) -> list[tuple[str, bytes, bytes]]:
    """Reads exact table bytes while decoding only names needed to select an entry."""
    stream.seek(list_offset)
    entries: list[tuple[str, bytes, bytes]] = []
    for index in range(file_count):
        raw = stream.read(14)
        if len(raw) != 14:
            raise AuthorError(f"PAK file table truncates at entry {index}")
        path_length = raw[0]
        obfuscation = raw[1] & PAK_ENTRY_TYPE_MASK
        stored_length = path_length if obfuscation == PAK_ENTRY_XTEA else path_length + 1
        raw_path = stream.read(stored_length)
        if len(raw_path) != stored_length:
            raise AuthorError(f"PAK path truncates at entry {index}")
        if obfuscation == PAK_ENTRY_XTEA:
            clear_path = decipher(key, raw_path).rstrip(b"\xcd\0")
        elif obfuscation == PAK_ENTRY_BASIC:
            clear_path = raw_path[:path_length]
        elif obfuscation == PAK_ENTRY_XOR:
            clear_path = bytes(value ^ 0x71 for value in raw_path[:path_length])
        else:
            raise AuthorError(f"PAK entry {index} has unsupported type {raw[1]:#04x}")
        name = clear_path.decode("euc-kr", errors="strict").replace("\\", "/")
        entries.append((name, raw, raw_path))
    return entries


def replace_pak_entry(
    base_pak: pathlib.Path,
    out_pak: pathlib.Path,
    entry_name: str,
    data: bytes,
    region: str,
) -> dict[str, int | str]:
    """Atomically rebuilds a PAK while replacing one entry with authored XTEA/LZ2 data.

    The original archive body stays byte-for-byte intact. The replacement payload is appended
    where the old file table began, then the original table is copied after it with only the
    selected fourteen-byte metadata record changed. This avoids unpacking proprietary assets and
    avoids the fixed-capacity limitation of editing an existing payload in place.
    """
    key = REGION_KEYS[region]
    packed = literal_lz2(data)
    if max(len(data), len(packed)) > 0xFFFF_FFFF:
        raise AuthorError("replacement is too large for the PAK format")
    try:
        source_size = base_pak.stat().st_size
        source_mode = base_pak.stat().st_mode
        source = base_pak.open("rb")
    except OSError as error:
        raise AuthorError(f"cannot open base PAK {base_pak}: {error}") from error

    with source:
        if source_size < PAK_TRAILER_BYTES:
            raise AuthorError("base PAK is shorter than its trailer")
        source.seek(-PAK_TRAILER_BYTES, os.SEEK_END)
        trailer = source.read(PAK_TRAILER_BYTES)
        list_offset, file_count, signature = struct.unpack("<IIB", trailer)
        if signature != PAK_SIGNATURE:
            raise AuthorError(f"base PAK has invalid signature {signature:#04x}")
        if list_offset > source_size - PAK_TRAILER_BYTES:
            raise AuthorError("base PAK file-list offset is outside the file")
        entries = _read_pak_table(source, list_offset, file_count, key)
        if source.tell() != source_size - PAK_TRAILER_BYTES:
            raise AuthorError("base PAK file table does not end at its trailer")

        matches = [index for index, (name, _, _) in enumerate(entries) if name == entry_name]
        if len(matches) != 1:
            raise AuthorError(
                f"base PAK must contain {entry_name!r} exactly once; found {len(matches)}"
            )
        replacement_offset = list_offset
        new_list_offset = replacement_offset + len(packed)
        if new_list_offset > 0xFFFF_FFFF:
            raise AuthorError("rebuilt PAK file-list offset exceeds u32")

        target = matches[0]
        old_raw = entries[target][1]
        path_type = old_raw[1] & PAK_ENTRY_TYPE_MASK
        if path_type != PAK_ENTRY_XTEA:
            raise AuthorError("retail replacement target must use XTEA metadata")
        raw = bytearray(
            struct.pack(
                "<BBIII",
                old_raw[0],
                PAK_ENTRY_XTEA_LZ2,
                replacement_offset,
                len(packed),
                len(data),
            )
        )
        metadata = encipher(key, bytes(raw[2:6] + raw[10:14]))
        raw[2:6] = metadata[:4]
        raw[10:14] = metadata[4:]

        out_pak.parent.mkdir(parents=True, exist_ok=True)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=out_pak.parent, delete=False) as output:
                temporary = pathlib.Path(output.name)
                source.seek(0)
                remaining = list_offset
                while remaining:
                    chunk = source.read(min(1024 * 1024, remaining))
                    if not chunk:
                        raise AuthorError("base PAK body truncates before its file table")
                    output.write(chunk)
                    remaining -= len(chunk)
                output.write(packed)
                for index, (_, entry_raw, raw_path) in enumerate(entries):
                    output.write(raw if index == target else entry_raw)
                    output.write(raw_path)
                output.write(struct.pack("<IIB", new_list_offset, file_count, PAK_SIGNATURE))
                output.flush()
                os.fsync(output.fileno())
            os.chmod(temporary, source_mode)
            os.replace(temporary, out_pak)
        except Exception:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
            raise

    return {
        "entry": entry_name,
        "entry_count": file_count,
        "old_list_offset": list_offset,
        "new_list_offset": new_list_offset,
        "replacement_offset": replacement_offset,
        "packed_size": len(packed),
        "real_size": len(data),
    }


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-archive", required=True, type=pathlib.Path)
    parser.add_argument("--catalog", required=True, type=pathlib.Path)
    parser.add_argument("--out-archive", required=True, type=pathlib.Path)
    parser.add_argument(
        "--out-pak",
        type=pathlib.Path,
        help="optional one-entry diagnostic/overlay PAK",
    )
    parser.add_argument(
        "--out-server-iff-dir",
        type=pathlib.Path,
        help="write server tables/manifest from this same authored archive",
    )
    parser.add_argument(
        "--replace-in-pak",
        type=pathlib.Path,
        help="retail PAK whose winning entry must be replaced",
    )
    parser.add_argument(
        "--out-client-pak",
        type=pathlib.Path,
        help="rebuilt manual-sync PAK (requires --replace-in-pak)",
    )
    parser.add_argument("--pak-entry", default="data/pangya_gb.iff")
    parser.add_argument("--region", default="us", choices=sorted(REGION_KEYS))
    parser.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()

    try:
        if (args.replace_in_pak is None) != (args.out_client_pak is None):
            raise AuthorError("--replace-in-pak and --out-client-pak must be provided together")
        managed, offers, invent = load_catalog(args.catalog)
        authored = author_archive(
            args.base_archive, args.out_archive, managed, offers, invent
        )
        authored_bytes = args.out_archive.read_bytes()
        if args.out_pak is not None:
            atomic_write(
                args.out_pak,
                build_retail_pak(args.pak_entry, authored_bytes, args.region),
            )
        server_iff = None
        if args.out_server_iff_dir is not None:
            server_iff = write_server_iff_directory(args.out_archive, args.out_server_iff_dir)
        replacement = None
        if args.replace_in_pak is not None:
            replacement = replace_pak_entry(
                args.replace_in_pak,
                args.out_client_pak,
                args.pak_entry,
                authored_bytes,
                args.region,
            )
        report = {
            "version": 1,
            "base_archive_sha256": sha256(args.base_archive),
            "authored_archive_sha256": sha256(args.out_archive),
            "authored_pak_sha256": sha256(args.out_pak) if args.out_pak is not None else None,
            "client_pak_sha256": (
                sha256(args.out_client_pak) if args.out_client_pak is not None else None
            ),
            # Which archive the authored tables were injected into.
            #
            # This is not cosmetic. The client loads the LAST archive in its series that provides
            # a given file, and `data/pangya_gb.iff` is present in nearly every one — so
            # authoring `projectg850gb.pak` in a stock U.S. 851 install changes nothing, because
            # `projectg851gb.pak` supplies a later copy that wins. Anything cross-checking a
            # deployment has to hash the archive that was actually written, not a guess.
            "client_pak_name": (
                args.out_client_pak.name if args.out_client_pak is not None else None
            ),
            "pak_replacement": replacement,
            "server_iff": server_iff,
            "managed_tables": managed,
            "offers": authored,
        }
        rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
        if args.report is not None:
            atomic_write(args.report, rendered.encode("utf-8"))
        print(rendered, end="")
        return 0
    except AuthorError as error:
        parser.error(str(error))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
