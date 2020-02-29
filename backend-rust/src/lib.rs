use std::thread::{JoinHandle, spawn};
use std::sync::mpsc::{channel, Receiver, Sender};

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
    tx: Sender<ListenerCommand>,
}

pub struct Tollbooth {
    listeners: Vec<WalletListener>,
}

impl Tollbooth {
    pub fn new(wallets: Vec<Wallet>) -> Self {
        let mut listeners = Vec::new();
        for wallet in wallets.into_iter() {
            let wallet_clone = wallet.clone();
            let (tx, rx) = channel();
            let listen_thread = spawn(move || {
                listen_loop(wallet_clone, rx);
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

    pub fn shutdown(self) {
        for listener in self.listeners {
            match listener.tx.send(ListenerCommand::Shutdown) {
                Ok(_) => {
                    match listener.listen_thread.join() {
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

fn listen_loop(_wallet: Wallet, _rx: Receiver<ListenerCommand>) {
}
