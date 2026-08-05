#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Thin `pangya-server` command-line entry point.

use std::time::Duration;

use clap::Parser as _;
use pangya_server::Cli;

const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

fn main() {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: Tokio runtime initialization failed: {error}");
            std::process::exit(1);
        }
    };
    let status = runtime.block_on(pangya_server::run(Cli::parse()));
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
    if let Err(error) = status {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
