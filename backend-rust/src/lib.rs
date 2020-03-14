#[macro_use]
extern crate log;
extern crate reqwest;
extern crate tokio;

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
                        Err(e) => error!("Listener thread for wallet {:?} panicked with {:?}", listener.wallet, e),
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

async fn make_request(wallet: &Wallet, reqwester: &reqwest::Client) -> Result<reqwest::Response, reqwest::Error> {
    let url = match &wallet {
        Wallet::StellarTest(pubkey) => format!("https://horizon-testnet.stellar.org/accounts/{}/payments", pubkey),
        Wallet::Stellar(pubkey) => format!("https://horizon.stellar.org/accounts/{}/payments", pubkey),
    };
    reqwester.get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .and_then(|r| r.error_for_status())
}

async fn listen_loop(wallet: Wallet, mut rx: UnboundedReceiver<ListenerCommand>) {
    let reqwester = reqwest::Client::new();
    let mut stream = match make_request(&wallet, &reqwester).await {
        Err(e) => {
            error!("Initial request to listen for wallet {:?} failed: {:?}", &wallet, e);
            return;
        }
        Ok(r) => r.bytes_stream(),
    };

    let mut retrying = false;
    let mut timer = tokio::time::delay_for(tokio::time::Duration::from_millis(0));
    let mut retry_fail_delay = 500;

    let mut shutdown = false;
    let mut buffer = Vec::<u8>::new();
    while !shutdown {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    None => {
                        warn!("Listener task seems to have been detached by dropped parent");
                        shutdown = true;
                    }
                    Some(ListenerCommand::Shutdown) => {
                        shutdown = true;
                    }
                }
            }
            _ = &mut timer, if retrying => {
                match make_request(&wallet, &reqwester).await {
                    Err(e) => {
                        if retry_fail_delay > 1000 {
                            error!("Maximum number of failed retries reached, aborting...");
                            shutdown = true;
                        } else {
                            retry_fail_delay = retry_fail_delay * 2;
                            warn!("Error on retry request for wallet {:?}, retrying again in {}ms: {:?}", &wallet, retry_fail_delay, e);
                            timer = tokio::time::delay_for(tokio::time::Duration::from_millis(retry_fail_delay));
                        }
                    }
                    Ok(r) => {
                        stream = r.bytes_stream();
                        info!("Successfully refreshed connection to server");
                        retrying = false;
                        retry_fail_delay = 500;
                    }
                };
            }
            item = stream.next() => {
                match item {
                    None => {
                        warn!("Unexpected stream termination for wallet {:?}, retrying connection", &wallet);
                        timer = tokio::time::delay_for(tokio::time::Duration::from_millis(0));
                        retrying = true;
                    }
                    Some(Err(e)) => {
                        warn!("Unexpected error while streaming payments for wallet {:?}: {:?}", &wallet, e);
                        timer = tokio::time::delay_for(tokio::time::Duration::from_millis(0));
                        retrying = true;
                    }
                    Some(Ok(bytes)) => {
                        buffer.extend_from_slice(&bytes);
                        while let Some(ix_newline) = buffer.iter().position(|b| *b == b'\n') {
                            let line = buffer.drain(0..ix_newline + 1).collect::<Vec<u8>>();
                            let line = match std::str::from_utf8(&line) {
                                Ok(t) => t.trim().to_string(),
                                Err(_) => {
                                    warn!("Error while UTF-8 decoding server-sent event; ignoring line [{}]", String::from_utf8_lossy(&line));
                                    continue;
                                }
                            };
                            let (key, value) = match line.find(':') {
                                Some(colon_ix) => (line[0..colon_ix].trim(), line[colon_ix + 1..].trim()),
                                None => (line.trim(), ""),
                            };
                            match key {
                                "retry" => {
                                    let delay : u64 = value.parse().unwrap_or(0);
                                    debug!("Got retry interval '{}' ({}), scheduling", value, delay);
                                    timer = tokio::time::delay_for(tokio::time::Duration::from_millis(delay));
                                    retrying = true;
                                }
                                "data" => println!("Got data {}", value),
                                _ => (),
                            };
                        }
                    }
                }
            }
        }
    }
}
