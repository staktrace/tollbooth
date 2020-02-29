extern crate tokio;

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

async fn listen_loop(_wallet: Wallet, mut rx: UnboundedReceiver<ListenerCommand>) {
    match rx.recv().await {
        Some(ListenerCommand::Shutdown) => (),
        None => eprintln!("Listener task seems to have been detached by dropped parent"),
    }
}
