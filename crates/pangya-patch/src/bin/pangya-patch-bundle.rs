//! Produces a signed changed-IFF-member release directory from operator-owned PAK inputs.
//! No PAK is copied to the output; it contains only manifest, signature, and changed members.

use clap::Parser;
use ed25519_dalek::SigningKey;
use pangya_patch::produce_release;
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    /// Pristine operator-owned projectg851gb.pak.
    #[arg(long)]
    base: PathBuf,
    /// Locally authored result PAK, never copied into output.
    #[arg(long)]
    result: PathBuf,
    /// Monotonically increasing release id.
    #[arg(long)]
    release_id: u64,
    /// Identifier matching launcher-pinned public key.
    #[arg(long)]
    key_id: String,
    /// 32-byte Ed25519 signing seed as hexadecimal. Never persisted by this tool.
    #[arg(long, env = "PANGYA_PATCH_SIGNING_SEED")]
    signing_seed: String,
    /// New or empty release directory.
    #[arg(long)]
    output: PathBuf,
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let seed = decode_seed(&args.signing_seed)?;
    if args.output.exists()
        && std::fs::read_dir(&args.output)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("output directory must be new or empty".to_owned());
    }
    let base = std::fs::read(&args.base).map_err(|e| format!("read base: {e}"))?;
    let result = std::fs::read(&args.result).map_err(|e| format!("read result: {e}"))?;
    // US PAK XTEA key, source: /Users/thunderjr/projects/pangya-rs/opensource-references/pangbox--pangfiles/crypto/pyxtea/keys.go:5-12.
    let key = [0x03f6_07a9, 0x036f_5a3e, 0x0110_02b4, 0x04ab_00ea];
    let (manifest, signature, payloads) = produce_release(
        &base,
        &result,
        key,
        args.release_id,
        args.key_id,
        &SigningKey::from_bytes(&seed),
    )
    .map_err(|e| e.to_string())?;
    write_release(
        &args.output,
        &manifest.canonical_json().map_err(|e| e.to_string())?,
        &signature,
        &payloads,
    )?;
    Ok(())
}

fn write_release(
    output: &std::path::Path,
    manifest: &[u8],
    signature: &[u8],
    payloads: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let payload_dir = output.join("payload");
    std::fs::create_dir_all(&payload_dir).map_err(|e| e.to_string())?;
    atomic_write(&output.join("release-manifest.json"), manifest)?;
    atomic_write(&output.join("release-manifest.json.sig"), signature)?;
    for (name, bytes) in payloads {
        atomic_write(&payload_dir.join(name), bytes)?;
    }
    Ok(())
}

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, path).map_err(|e| e.to_string())
}

fn decode_seed(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("signing seed must be 32-byte hexadecimal".to_owned());
    }
    let mut seed = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        };
        seed[index] = digit(pair[0])
            .and_then(|high| digit(pair[1]).map(|low| high * 16 + low))
            .ok_or_else(|| "signing seed must be hexadecimal".to_owned())?;
    }
    Ok(seed)
}
