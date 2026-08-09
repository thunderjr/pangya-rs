from __future__ import annotations

import importlib.util
import json
import pathlib
import struct
import sys
import tempfile
import unittest
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[2]


def load_script(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


author = load_script("author_client_iff", ROOT / "scripts" / "author-client-iff.py")
extract = load_script("extract_client_iff", ROOT / "scripts" / "extract-client-iff.py")


def retail_pak(entries: list[tuple[str, bytes]]) -> bytes:
    key = author.REGION_KEYS["us"]
    body = bytearray()
    table_data = bytearray()
    for name, data in entries:
        offset = len(body)
        packed = author.literal_lz2(data)
        body.extend(packed)
        path = author.encode_pak_path(name)
        padded = path + b"\xcd" * ((-len(path)) % 8)
        encrypted_path = author.encipher(key, padded)
        raw = bytearray(
            struct.pack(
                "<BBIII",
                len(encrypted_path),
                author.PAK_ENTRY_XTEA_LZ2,
                offset,
                len(packed),
                len(data),
            )
        )
        metadata = author.encipher(key, bytes(raw[2:6] + raw[10:14]))
        raw[2:6] = metadata[:4]
        raw[10:14] = metadata[4:]
        table_data.extend(raw)
        table_data.extend(encrypted_path)
    return bytes(body + table_data + struct.pack("<IIB", len(body), len(entries), 0x12))


def table(rows: list[tuple[int, int, int]]) -> bytes:
    record_size = 0x70
    data = bytearray(struct.pack("<HHI", len(rows), 13, 1))
    for type_id, price, flag in rows:
        row = bytearray(record_size)
        struct.pack_into("<I", row, 0, 1)
        struct.pack_into("<I", row, author.TYPE_ID_OFFSET, type_id)
        struct.pack_into("<I", row, author.PRICE_OFFSET, price)
        row[author.SHOP_FLAG_OFFSET] = flag
        data.extend(row)
    return bytes(data)


class AuthorClientIffTests(unittest.TestCase):
    def make_source(self, root: pathlib.Path, rows: list[tuple[int, int, int]]) -> pathlib.Path:
        source = root / "base.iff"
        with zipfile.ZipFile(source, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("ClubSet.iff", table(rows))
            archive.writestr("Untouched.iff", b"opaque")
        return source

    def test_authors_only_declared_offers_and_builds_readable_basic_pak(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            source = self.make_source(
                root,
                [(0x10000001, 5000, 0x20), (0x10000002, 6000, 0x21)],
            )
            catalog = root / "catalog.json"
            catalog.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "managed_tables": ["ClubSet.iff"],
                        "offers": [
                            {"table": "ClubSet.iff", "type_id": "0x10000002", "pang": 1234}
                        ],
                    }
                ),
                encoding="utf-8",
            )
            managed, offers = author.load_catalog(catalog)
            destination = root / "custom.iff"
            report = author.author_archive(source, destination, managed, offers)
            self.assertEqual(report[0]["type_id"], "0x10000002")

            with zipfile.ZipFile(destination) as archive:
                authored = archive.read("ClubSet.iff")
                self.assertEqual(archive.read("Untouched.iff"), b"opaque")
            first = author.HEADER_BYTES
            second = first + 0x70
            self.assertEqual(authored[first + author.SHOP_FLAG_OFFSET], 0)
            self.assertEqual(
                struct.unpack_from("<I", authored, first + author.PRICE_OFFSET)[0],
                author.CLIENT_UNAVAILABLE_PRICE,
            )
            self.assertEqual(
                authored[second + author.SHOP_FLAG_OFFSET],
                0x20,
                "a Points row is converted to the non-tradeable Pang form",
            )
            self.assertEqual(struct.unpack_from("<I", authored, second + author.PRICE_OFFSET)[0], 1234)

            pak = author.build_basic_pak("data/pangya_gb.iff", destination.read_bytes())
            data_size = len(destination.read_bytes())
            path_size = len(b"data/pangya_gb.iff")
            self.assertEqual(pak[data_size + 14 + path_size], 0)
            entries = extract.read_file_table(pak, extract.REGION_KEYS["us"])
            self.assertEqual(entries[0][0], "data/pangya_gb.iff")
            self.assertEqual(extract.entry_bytes(pak, entries[0][1]), destination.read_bytes())

            # A production run writes the server tables from this exact authored ZIP, never
            # from a separately edited copy.
            for required in author.SERVER_TABLE_KINDS:
                if required == "ClubSet.iff":
                    continue
                with zipfile.ZipFile(destination, "a", zipfile.ZIP_DEFLATED) as archive:
                    archive.writestr(required, table([(0x10000003, 1, 0x20)]))
            server_dir = root / "server-iff"
            server_report = author.write_server_iff_directory(destination, server_dir)
            self.assertEqual((server_dir / "ClubSet.iff").read_bytes(), authored)
            self.assertEqual(len(server_report["files"]), len(author.SERVER_TABLE_KINDS))
            self.assertIn("manifest_version = 3", (server_dir / "manifest.toml").read_text())

            retail_pak = author.build_retail_pak(
                "data/pangya_gb.iff", destination.read_bytes(), "us"
            )
            retail_entries = extract.read_file_table(retail_pak, extract.REGION_KEYS["us"])
            self.assertEqual(retail_entries[0][0], "data/pangya_gb.iff")
            self.assertEqual(retail_entries[0][1]["type"], author.PAK_ENTRY_XTEA_LZ2)
            self.assertEqual(
                extract.entry_bytes(retail_pak, retail_entries[0][1]), destination.read_bytes()
            )

    def test_rebuilds_the_winning_pak_without_touching_other_entries(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            source = root / "base.pak"
            original = retail_pak(
                [
                    ("data/pangya_gb.iff", b"old catalog"),
                    ("data/opaque.bin", b"must survive byte-for-byte"),
                ]
            )
            source.write_bytes(original)
            original_list_offset = struct.unpack_from("<I", original, len(original) - 9)[0]
            destination = root / "rebuilt.pak"
            report = author.replace_pak_entry(
                source,
                destination,
                "data/pangya_gb.iff",
                b"new authored catalog",
                "us",
            )

            rebuilt = destination.read_bytes()
            self.assertEqual(rebuilt[:original_list_offset], original[:original_list_offset])
            self.assertEqual(report["old_list_offset"], original_list_offset)
            self.assertGreater(report["new_list_offset"], original_list_offset)
            entries = extract.read_file_table(rebuilt, extract.REGION_KEYS["us"])
            decoded = {name: extract.entry_bytes(rebuilt, metadata) for name, metadata in entries}
            self.assertEqual(decoded["data/pangya_gb.iff"], b"new authored catalog")
            self.assertEqual(decoded["data/opaque.bin"], b"must survive byte-for-byte")

            missing = root / "missing.pak"
            with self.assertRaisesRegex(author.AuthorError, "exactly once"):
                author.replace_pak_entry(
                    source,
                    missing,
                    "data/not-present.iff",
                    b"replacement",
                    "us",
                )
            self.assertFalse(missing.exists())

    def test_refuses_to_invent_shop_metadata_for_an_unsold_row(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            source = self.make_source(root, [(0x10000001, 5000, 0)])
            catalog = root / "catalog.json"
            catalog.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "managed_tables": ["ClubSet.iff"],
                        "offers": [
                            {"table": "ClubSet.iff", "type_id": 0x10000001, "pang": 1}
                        ],
                    }
                ),
                encoding="utf-8",
            )
            managed, offers = author.load_catalog(catalog)
            with self.assertRaisesRegex(author.AuthorError, "refusing to invent"):
                author.author_archive(source, root / "custom.iff", managed, offers)


if __name__ == "__main__":
    unittest.main()
