use std::process::ExitCode;

use story_pointing::{cli, server};
use tokio::net::TcpListener;

/// Listens on `--bind ADDR` / `--bind=ADDR`, else `BIND`, else `127.0.0.1:8080`,
/// prints `listening on <addr>` with the bound address (the real port when given
/// port 0) and serves until killed.
#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env_bind = std::env::var("BIND").ok();
    let addr = match cli::address_from(&args, env_bind.as_deref()) {
        Ok(addr) => addr,
        Err(cli::CliError::MissingBindValue) => {
            eprintln!("--bind needs an address, as in --bind 127.0.0.1:8080");
            return ExitCode::from(2);
        }
        Err(cli::CliError::UnexpectedArgument(arg)) => {
            eprintln!("unexpected argument {arg:?}; usage: story-pointing [--bind ADDR]");
            return ExitCode::from(2);
        }
    };
    let listener = match TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("cannot listen on {addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    match listener.local_addr() {
        Ok(local) => println!("listening on {local}"),
        Err(_) => println!("listening on {addr}"),
    }
    if let Err(err) = server::serve(listener).await {
        eprintln!("server stopped: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
