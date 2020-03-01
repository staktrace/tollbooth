extern crate tokio;

use std::env;
use tollbooth_backend::{Tollbooth, Wallet};

fn help() {
    eprintln!("Please provide wallets to listen for as command line arguments. Supported wallets:");
    eprintln!("  stellar:test:<publickey>");
    eprintln!("  stellar:<publickey>");
}

#[tokio::main]
async fn main() {
    let mut wallets = Vec::new();
    for arg in env::args().skip(1) {
        if arg.starts_with("stellar:test:") {
            wallets.push(Wallet::StellarTest(arg[13..].to_string()));
        } else if arg.starts_with("stellar:") {
            wallets.push(Wallet::Stellar(arg[8..].to_string()));
        } else {
            panic!("Unrecognized wallet identifier {}. Run without arguments to get help", arg);
        }
    }
    if wallets.is_empty() {
        eprintln!("No wallets specified on the command line. Exiting!");
        help();
        return;
    }

    let tb = Tollbooth::new(wallets);
    eprintln!("Setup complete, waiting for events... (use ctrl-c to shutdown)");
    tokio::signal::ctrl_c().await.expect("Error listening for ctrl-c signal");
    eprintln!("ctrl-c received, shutting down...");
    tb.shutdown().await;
    eprintln!("Clean shutdown complete.");
}
