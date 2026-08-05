//! Real-PostgreSQL black-box account CLI secret-source coverage.

use std::{
    io::Write as _,
    process::{Command, Stdio},
};

const SECRET: &str = "abcdefabcdefabcdefabcdefabcdefab";

fn base_command(username: &str, nickname: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pangya-server"));
    let config =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/local.example.toml");
    command.args([
        "--config",
        config.to_str().expect("config path"),
        "account",
        "create",
        "--username",
        username,
        "--nickname",
        nickname,
    ]);
    command
}

fn assert_output_and_audit(output: std::process::Output, username: &str) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = [output.stdout, output.stderr].concat();
    assert!(
        !combined
            .windows(SECRET.len())
            .any(|bytes| bytes == SECRET.as_bytes())
    );
    assert!(String::from_utf8_lossy(&combined).contains("account created: id="));

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let count = runtime.block_on(async move {
        let pool = sqlx::PgPool::connect(&database_url).await.expect("connect");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM operator_audit_events e \
             JOIN accounts a ON a.id = e.account_id \
             WHERE e.action = 'account_create' AND e.outcome = 'success' \
               AND a.username_normalized = $1",
        )
        .bind(username)
        .fetch_one(&pool)
        .await
        .expect("audit query");
        pool.close().await;
        count
    });
    assert_eq!(count, 1);
}

#[test]
fn account_create_reads_secret_from_stdin_without_echo() {
    let suffix = std::process::id();
    let username = format!("cli{suffix}a");
    let mut child = base_command(&username, &format!("nick{suffix}a"))
        .arg("--secret-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn account CLI");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(format!("{SECRET}\n").as_bytes())
        .expect("write secret");
    assert_output_and_audit(child.wait_with_output().expect("CLI output"), &username);
}

#[test]
fn oversized_stdin_secret_is_rejected_without_echo() {
    let suffix = std::process::id();
    let oversized = vec![b'a'; 129];
    let mut child = base_command(&format!("cli{suffix}y"), &format!("nick{suffix}y"))
        .arg("--secret-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn account CLI");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&oversized)
        .expect("write oversized secret");
    let output = child.wait_with_output().expect("CLI output");
    assert!(!output.status.success());
    let combined = [output.stdout, output.stderr].concat();
    assert!(
        !combined
            .windows(oversized.len())
            .any(|bytes| bytes == oversized)
    );
}

#[test]
fn account_create_reads_secret_from_named_environment_variable() {
    let suffix = std::process::id();
    let username = format!("cli{suffix}b");
    let output = base_command(&username, &format!("nick{suffix}b"))
        .args(["--secret-env", "PANGYA_CLI_TEST_SECRET"])
        .env("PANGYA_CLI_TEST_SECRET", SECRET)
        .output()
        .expect("account CLI");
    assert_output_and_audit(output, &username);
}

#[test]
fn oversized_mounted_secret_file_is_rejected_without_echo() {
    let suffix = std::process::id();
    let secret_file = std::env::temp_dir().join(format!("pangya-cli-oversized-secret-{suffix}"));
    let oversized = "a".repeat(129);
    std::fs::write(&secret_file, &oversized).expect("secret file");
    let output = base_command(&format!("cli{suffix}z"), &format!("nick{suffix}z"))
        .arg("--secret-file")
        .arg(&secret_file)
        .output()
        .expect("account CLI");
    std::fs::remove_file(secret_file).expect("remove secret file");
    assert!(!output.status.success());
    let combined = [output.stdout, output.stderr].concat();
    assert!(!String::from_utf8_lossy(&combined).contains(&oversized));
}

#[test]
fn invalid_utf8_mounted_secret_file_is_rejected_without_echo() {
    let suffix = std::process::id();
    let secret_file = std::env::temp_dir().join(format!("pangya-cli-invalid-secret-{suffix}"));
    let invalid = [0xff, 0xfe, 0xfd];
    std::fs::write(&secret_file, invalid).expect("secret file");
    let output = base_command(&format!("cli{suffix}x"), &format!("nick{suffix}x"))
        .arg("--secret-file")
        .arg(&secret_file)
        .output()
        .expect("account CLI");
    std::fs::remove_file(secret_file).expect("remove secret file");
    assert!(!output.status.success());
    let combined = [output.stdout, output.stderr].concat();
    assert!(
        !combined
            .windows(invalid.len())
            .any(|bytes| bytes == invalid)
    );
}

#[test]
fn account_create_reads_secret_from_mounted_file_without_echo() {
    let suffix = std::process::id();
    let username = format!("cli{suffix}c");
    let secret_file = std::env::temp_dir().join(format!("pangya-cli-secret-{suffix}"));
    std::fs::write(&secret_file, SECRET).expect("secret file");
    let output = base_command(&username, &format!("nick{suffix}c"))
        .arg("--secret-file")
        .arg(&secret_file)
        .output()
        .expect("account CLI");
    std::fs::remove_file(secret_file).expect("remove secret file");
    assert_output_and_audit(output, &username);
}
