#!/usr/bin/env python3
"""Extract the winning ``pangya_*.iff`` archive from a PangYa client's PAK chain.

The client does not read its item tables from a loose file. It resolves them through its PAK
chain, where a later PAK supersedes a same-named entry in an earlier one, so the copy that
matters is whichever the newest PAK provides. A catalog built from an older copy loads and
validates cleanly and is still wrong: it silently lacks every item added since, and the symptom
is a purchase refused for an item id that sits inside a family's range but is absent from the
table.

This walks the PAKs in name order, applies that overlay, and writes the winning archive out.

Format from ``pangbox/pangfiles`` (``pak/format.go``, ``pak/reader.go``), ISC licensed. The
cipher is PangYa's XTEA variant, sixteen rounds with the delta subtracted while encrypting, and
the per-region keys come from ``crypto/pyxtea/keys.go``.

Usage:
    scripts/extract-client-iff.py --client-dir /path/to/client --out local-data/us851-data/pak-iff
    scripts/extract-client-iff.py --client-dir ... --region us --list
"""

from __future__ import annotations

import argparse
import pathlib
import struct
import sys

TRAILER_LEN = 9
TRAILER_SIGNATURE = 0x12

FILE_TYPE_MASK = 0x0F
FILE_TYPE_BASIC = 0x00
FILE_TYPE_LZ = 0x01
FILE_TYPE_DIR = 0x02
FILE_TYPE_LZ2 = 0x03

ENTRY_TYPE_MASK = 0xF0
ENTRY_TYPE_XOR = 0x10
ENTRY_TYPE_XTEA = 0x20
ENTRY_TYPE_BASIC = 0x80

XTEA_ROUNDS = 16
XTEA_DELTA = 0x61C88647
XTEA_INITIAL_SUM = 0xE3779B90
MASK32 = 0xFFFFFFFF

# crypto/pyxtea/keys.go
REGION_KEYS = {
    "us": (0x03F607A9, 0x036F5A3E, 0x011002B4, 0x04AB00EA),
    "jp": (0x020A5FD4, 0x01EEBDFF, 0x02B3C6A0, 0x04F6A3E1),
    "th": (0x050AD33B, 0x00BAFF09, 0x0452FFDA, 0x02CB4422),
    "eu": (0x01E986D8, 0x05818479, 0x03D2B0BB, 0x02C9B030),
    "id": (0x01640DB7, 0x01455A9B, 0x027F1AB7, 0x05918B54),
    "kr": (0x0485B576, 0x05148E02, 0x05141D96, 0x028FA9D6),
}


def decrypt_block(key: tuple[int, int, int, int], block: bytes) -> bytes:
    data0, data1 = struct.unpack("<II", block)
    total = XTEA_INITIAL_SUM
    for _ in range(XTEA_ROUNDS):
        mixed = (((data0 << 4) & MASK32) ^ (data0 >> 5)) + data0
        data1 = (data1 - ((mixed & MASK32) ^ ((total + key[(total >> 11) & 3]) & MASK32))) & MASK32
        total = (total + XTEA_DELTA) & MASK32
        mixed = (((data1 << 4) & MASK32) ^ (data1 >> 5)) + data1
        data0 = (data0 - ((mixed & MASK32) ^ ((total + key[total & 3]) & MASK32))) & MASK32
    return struct.pack("<II", data0, data1)


def decipher(key: tuple[int, int, int, int], data: bytearray) -> None:
    """Deciphers whole blocks in place; a trailing partial block is left alone, as upstream does."""
    for offset in range(0, len(data) - (len(data) % 8), 8):
        data[offset : offset + 8] = decrypt_block(key, bytes(data[offset : offset + 8]))


def read_file_table(blob: bytes, key: tuple[int, int, int, int]) -> list[tuple[str, dict]]:
    if len(blob) < TRAILER_LEN:
        raise ValueError("file is shorter than a PAK trailer")
    list_offset, file_count, signature = struct.unpack("<IIB", blob[-TRAILER_LEN:])
    if signature != TRAILER_SIGNATURE:
        raise ValueError(f"bad PAK signature {signature:#04x}")

    entries: list[tuple[str, dict]] = []
    cursor = list_offset
    for index in range(file_count):
        raw = bytearray(blob[cursor : cursor + 14])
        if len(raw) != 14:
            raise ValueError(f"truncated entry {index}")
        cursor += 14

        # Entry metadata is XTEA'd across two disjoint four-byte runs.
        if raw[1] & ENTRY_TYPE_MASK == ENTRY_TYPE_XTEA:
            scratch = bytearray(raw[2:6] + raw[10:14])
            decipher(key, scratch)
            raw[2:6] = scratch[0:4]
            raw[10:14] = scratch[4:8]

        path_length, entry_type, offset, packed_size, real_size = struct.unpack("<BBIII", raw)
        if entry_type & ENTRY_TYPE_MASK == 0:
            entry_type |= ENTRY_TYPE_XOR

        obfuscation = entry_type & ENTRY_TYPE_MASK
        if obfuscation == ENTRY_TYPE_XOR:
            raw_path = bytearray(blob[cursor : cursor + path_length + 1])
            cursor += path_length + 1
            real_size ^= 0x71
            path = bytes(byte ^ 0x71 for byte in raw_path[:path_length])
        elif obfuscation == ENTRY_TYPE_XTEA:
            raw_path = bytearray(blob[cursor : cursor + path_length])
            cursor += path_length
            decipher(key, raw_path)
            path = bytes(raw_path).rstrip(b"\xcd\x00")
        else:
            raw_path = blob[cursor : cursor + path_length]
            cursor += path_length
            path = bytes(raw_path).rstrip(b"\x00")

        name = path.decode("euc-kr", errors="replace").replace("\\", "/")
        entries.append(
            (
                name,
                {
                    "type": entry_type,
                    "offset": offset,
                    "packed_size": packed_size,
                    "real_size": real_size,
                },
            )
        )
    return entries


# pak/decompress.go: the obfuscated LZ variant xors each back-reference with one of these,
# selected by the top bits of the untouched control byte.
VALUE_PAD = (0xFF21, 0x834F, 0x675F, 0x0034, 0xF237, 0x815F, 0x4765, 0x0233)


def decompress(blob: bytes, entry: dict) -> bytes:
    """Decompresses one entry, mirroring ``pak/decompress.go``.

    A control byte introduces each group of eight tokens: a clear bit copies one literal, a set
    bit copies a back-reference. The LZ2 variant obfuscates both the control byte and the
    reference words, which is the only difference between the two compressed types.
    """
    kind = entry["type"] & FILE_TYPE_MASK
    start = entry["offset"]
    packed = entry["packed_size"]
    if kind == FILE_TYPE_BASIC:
        return blob[start : start + packed]

    out = bytearray()
    counter = 0
    sequence = 0
    raw_sequence = 0
    cursor = 0
    while cursor < packed:
        if counter == 0:
            sequence = blob[start + cursor]
            raw_sequence = sequence
            cursor += 1
            if kind == FILE_TYPE_LZ2:
                sequence ^= 0xC8
        else:
            sequence >>= 1

        if sequence & 1:
            value = int.from_bytes(blob[start + cursor : start + cursor + 2], "little")
            cursor += 2
            if kind == FILE_TYPE_LZ2:
                value ^= VALUE_PAD[(raw_sequence >> 3) & 7]
            distance = value & 0xFFF
            length = (value >> 12) + 2
            # Upstream grows the output first and then copies relative to the *new* end, which
            # makes the source simply `distance` bytes back from where the run starts.
            source = len(out) - distance
            if source < 0:
                raise ValueError("back-reference precedes the start of the output")
            # Copied one byte at a time: runs may legitimately overlap themselves.
            for index in range(length):
                out.append(out[source + index])
        else:
            out.append(blob[start + cursor])
            cursor += 1
        counter = (counter + 1) & 7
    return bytes(out)


def entry_bytes(blob: bytes, entry: dict) -> bytes | None:
    """Returns the entry's bytes, or None for a directory entry."""
    if entry["type"] & FILE_TYPE_MASK == FILE_TYPE_DIR:
        return None
    return decompress(blob, entry)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client-dir", required=True, type=pathlib.Path)
    parser.add_argument("--out", type=pathlib.Path)
    parser.add_argument("--region", default="us", choices=sorted(REGION_KEYS))
    parser.add_argument(
        "--name",
        default="pangya_gb.iff",
        help="archive to extract; the client tries several names in order",
    )
    parser.add_argument("--list", action="store_true", help="only report which PAKs provide it")
    args = parser.parse_args()

    paks = sorted(args.client_dir.glob("*.pak"))
    if not paks:
        print(f"no .pak files under {args.client_dir}", file=sys.stderr)
        return 1
    key = REGION_KEYS[args.region]

    winner: tuple[pathlib.Path, dict, bytes] | None = None
    for pak in paks:
        blob = pak.read_bytes()
        try:
            entries = read_file_table(blob, key)
        except ValueError as error:
            print(f"  {pak.name}: unreadable ({error})", file=sys.stderr)
            continue
        for name, entry in entries:
            if pathlib.PurePosixPath(name).name.lower() != args.name.lower():
                continue
            kind = entry["type"] & FILE_TYPE_MASK
            print(f"  {pak.name}: {name} real={entry['real_size']} packed={entry['packed_size']} type={kind}")
            # Later PAKs supersede earlier ones, so simply keep overwriting.
            winner = (pak, entry, blob)

    if winner is None:
        print(f"{args.name} is not present in any PAK", file=sys.stderr)
        return 1
    pak, entry, blob = winner
    print(f"winning copy: {pak.name} ({entry['real_size']} bytes)")
    if args.list:
        return 0
    if args.out is None:
        print("pass --out to write it", file=sys.stderr)
        return 1
    data = entry_bytes(blob, entry)
    if data is None:
        print("entry is a directory, not a file", file=sys.stderr)
        return 1
    if len(data) != entry["real_size"]:
        print(
            f"decompressed {len(data)} bytes but the table declares {entry['real_size']}",
            file=sys.stderr,
        )
        return 1
    args.out.mkdir(parents=True, exist_ok=True)
    target = args.out / args.name
    target.write_bytes(data)
    print(f"wrote {target} ({len(data)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
