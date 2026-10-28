use std::{path::Path, sync::mpsc};

use rayon::prelude::*;
use solana_snapshot_etl::{append_vec_iter, archived::ArchiveSnapshotExtractor, SnapshotExtractor};
use step_ingestooor_parsers::state_source::{AccountProvider, AccountProviderAccount};

use crate::ACCT_READ_BUFFER_SIZE;

#[derive(Clone, Debug)]
pub struct ArchiveSnapshotAccountProvider {
    pub(crate) snapshot_path: String,
    pub(crate) block_time: i64,
    pub(crate) threads: usize,
}

impl AccountProvider for ArchiveSnapshotAccountProvider {
    fn get_block_time(&self) -> i64 {
        self.block_time
    }

    fn all_accounts(&self) -> impl Iterator<Item = AccountProviderAccount> {
        let (tx, rx) = mpsc::sync_channel(ACCT_READ_BUFFER_SIZE);

        let threads = self.threads;
        log::info!("Using {} threads to read accounts", threads);
        let path = self.snapshot_path.clone();

        for i in 0..threads {
            let tx = tx.clone();
            let path = path.clone();

            std::thread::spawn(
                #[allow(clippy::unwrap_used)]
                move || {
                    let mut extractor = ArchiveSnapshotExtractor::open(Path::new(&path)).unwrap();
                    // for multithreading, we need to skip the first i elements, then step by the number of threads
                    let av_iters = extractor
                        .iter()
                        .filter_map(|a| a.ok())
                        .skip(i)
                        .step_by(threads);

                    for av_iter in av_iters {
                        let accts = append_vec_iter(&av_iter).par_bridge();
                        accts
                            .filter_map(|a| {
                                a.access().map(|b| AccountProviderAccount {
                                    pubkey: b.meta.pubkey,
                                    account: b.clone_account(),
                                })
                            })
                            .for_each_with(tx.clone(), |tx, acct| {
                                // if all consumers have been dropped, this will err
                                let _ = tx.send(acct);
                            });
                    }
                },
            );
        }

        rx.into_iter()
    }
}
