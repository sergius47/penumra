use std::{fs::File, io::Write, path::PathBuf};

use anyhow::{anyhow, Context as _, Result};
use directories::ProjectDirs;
use penumbra_crypto::{keys::SpendSeed, CURRENT_CHAIN_ID};
use penumbra_wallet::{ClientState, Wallet};
use rand_core::OsRng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use structopt::StructOpt;
use tempfile::NamedTempFile;

use crate::ClientStateFile;

#[derive(Debug, StructOpt)]
pub enum WalletCmd {
    /// Import an existing spend seed.
    Import {
        /// A 32-byte hex string encoding the spend seed.
        spend_seed: String,
    },
    /// Export the spend seed for the wallet.
    Export,
    /// Generate a new spend seed.
    Generate,
    /// Keep the spend seed, but reset all other client state.
    Reset,
    /// Delete the entire wallet permanently.
    Delete,
}

impl WalletCmd {
    /// Determine if this command requires a network sync before it executes.
    pub fn needs_sync(&self) -> bool {
        match self {
            WalletCmd::Import { .. } => false,
            WalletCmd::Export => false,
            WalletCmd::Generate => false,
            WalletCmd::Reset => false,
            WalletCmd::Delete => false,
        }
    }

    pub fn exec(&self, wallet_path: PathBuf) -> Result<()> {
        // Dispatch on the wallet command and return a new state if the command required a
        // wallet state to be saved to disk
        let state = match self {
            // These two commands return new wallets to be saved to disk:
            WalletCmd::Generate => Some(ClientState::new(Wallet::generate(&mut OsRng))),
            WalletCmd::Import { spend_seed } => {
                let seed = hex::decode(spend_seed)?;
                let seed = SpendSeed::try_from(seed.as_slice())?;
                Some(ClientState::new(Wallet::import(seed)))
            }
            // The rest of these commands don't require a wallet state to be saved to disk:
            WalletCmd::Export => {
                let state = ClientStateFile::load(wallet_path.clone())?;
                let seed = state.wallet().spend_key().seed().clone();
                println!("{}", hex::encode(&seed.0));
                None
            }
            WalletCmd::Delete => {
                if wallet_path.is_file() {
                    std::fs::remove_file(&wallet_path)?;
                    println!("Deleted wallet file at {}", wallet_path.display());
                } else if wallet_path.exists() {
                    return Err(anyhow!(
                            "Expected wallet file at {} but found something that is not a file; refusing to delete it",
                            wallet_path.display()
                        ));
                } else {
                    return Err(anyhow!(
                        "No wallet exists at {}, so it cannot be deleted",
                        wallet_path.display()
                    ));
                }
                None
            }
            WalletCmd::Reset => {
                tracing::info!("resetting client state");

                #[derive(Deserialize)]
                struct MinimalState {
                    wallet: Wallet,
                }

                // Read the wallet field out of the state file, without fully deserializing the rest
                let wallet =
                    serde_json::from_reader::<_, MinimalState>(File::open(&wallet_path)?)?.wallet;

                // Write the new wallet JSON to disk as a temporary file
                let (mut tmp, tmp_path) = NamedTempFile::new()?.into_parts();
                tmp.write_all(serde_json::to_string_pretty(&ClientState::new(wallet))?.as_bytes())?;

                // Check that we can successfully parse the result from disk
                ClientStateFile::load(tmp_path.to_path_buf()).context("can't parse wallet after attempting to reset: refusing to overwrite existing wallet file")?;

                // Move the temporary file over the original wallet file
                tmp_path.persist(&wallet_path)?;

                None
            }
        };

        // If a new wallet should be saved to disk, save it and also archive it in the archive directory
        if let Some(state) = state {
            // Never overwrite a wallet that already exists
            if wallet_path.exists() {
                return Err(anyhow::anyhow!(
                    "Wallet path {} already exists, refusing to overwrite it",
                    wallet_path.display()
                ));
            }

            println!("Saving wallet to {}", wallet_path.display());
            ClientStateFile::save(state.clone(), wallet_path)?;

            // Archive the newly generated state
            let archive_dir = ProjectDirs::from("zone", "penumbra", "penumbra-testnet-archive")
                .expect("can access penumbra-testnet-archive dir");

            // Create the directory <data dir>/penumbra-testnet-archive/<chain id>/<spend key hash prefix>/
            let spend_key_hash = Sha256::digest(&state.wallet().spend_key().seed().0);
            let wallet_archive_dir = archive_dir
                .data_dir()
                .join(CURRENT_CHAIN_ID)
                .join(hex::encode(&spend_key_hash[0..8]));
            std::fs::create_dir_all(&wallet_archive_dir)
                .expect("can create penumbra wallet archive directory");

            // Save the wallet file in the archive directory
            let archive_path = wallet_archive_dir.join("penumbra_wallet.json");
            println!("Saving backup wallet to {}", archive_path.display());
            ClientStateFile::save(state, archive_path)?;
        }

        Ok(())
    }
}