extern crate tokio;
extern crate reqwest;

use futures_util::stream::StreamExt;
use tokio::task::JoinHandle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone)]
pub enum Wallet {
    Stellar(String),
    StellarTest(String),
}

enum ListenerCommand {
    Shutdown,
}

struct WalletListener {
    listen_thread: JoinHandle<()>,
    wallet: Wallet,
    tx: UnboundedSender<ListenerCommand>,
}

pub struct Tollbooth {
    listeners: Vec<WalletListener>,
}

impl Tollbooth {
    pub fn new(wallets: Vec<Wallet>) -> Self {
        let mut listeners = Vec::new();
        for wallet in wallets.into_iter() {
            let wallet_clone = wallet.clone();
            let (tx, rx) = unbounded_channel();
            let listen_thread = tokio::spawn(async move {
                listen_loop(wallet_clone, rx).await;
            });
            listeners.push(WalletListener {
                listen_thread,
                wallet,
                tx
            });
        }
        Self {
            listeners,
        }
    }

    pub async fn shutdown(self) {
        for listener in self.listeners {
            match listener.tx.send(ListenerCommand::Shutdown) {
                Ok(_) => {
                    match listener.listen_thread.await {
                        Ok(_) => (),
                        Err(e) => eprintln!("Warning: listener thread for wallet {:?} panicked with {:?}", listener.wallet, e),
                    };
                }
                Err(_) => {
                    // thread already shut down
                    continue;
                }
            }
        }
    }
}

async fn listen_loop(wallet: Wallet, mut rx: UnboundedReceiver<ListenerCommand>) {
    let url = match &wallet {
        Wallet::StellarTest(pubkey) => format!("https://horizon-testnet.stellar.org/accounts/{}/payments", pubkey),
        Wallet::Stellar(pubkey) => format!("https://horizon.stellar.org/accounts/{}/payments", pubkey),
    };
    let req = reqwest::Client::new()
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .and_then(|r| r.error_for_status());
    let mut stream = match req {
        Err(e) => {
            eprintln!("Error on initial request for wallet {:?}: {:?}", &wallet, e);
            return;
        }
        Ok(r) => r.bytes_stream(),
    };

    let mut shutdown = false;
    while !shutdown {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    None => {
                        eprintln!("Listener task seems to have been detached by dropped parent");
                        shutdown = true;
                    }
                    Some(ListenerCommand::Shutdown) => {
                        shutdown = true;
                    }
                }
            }
            item = stream.next() => {
                match item {
                    None => {
                        eprintln!("Stream termination for wallet {:?}", &wallet);
                        // TODO: restart with new request
                        shutdown = true;
                    }
                    Some(Err(e)) => {
                        eprintln!("Error while streaming payments for wallet {:?}: {:?}", &wallet, e);
                        // TODO: restart with new request
                        shutdown = true;
                    }
                    Some(Ok(bytes)) => {
                        println!("Chunk: {:?}", bytes);
                    }
                }
            }
        }
    }
}
