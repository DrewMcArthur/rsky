use crate::common;
use crate::db::establish_connection;
use crate::repo::aws::s3::S3BlobStore;
use crate::repo::blob::BlobReader;
use crate::repo::preference::PreferenceReader;
use crate::repo::record::RecordReader;
use crate::repo::types::{
    write_to_op, CommitData, PreparedCreateOrUpdate, PreparedWrite, RecordCreateOrUpdateOp,
    RecordWriteEnum, RecordWriteOp, WriteOpAction,
};
use crate::storage::SqlRepoReader;
use anyhow::{bail, Result};
use diesel::*;
use futures::stream::{self, StreamExt};
use futures::try_join;
use libipld::Cid;
use rsky_crypto::secp256k1::keypair::{Secp256k1Keypair, Secp256k1KeypairOptions};
use rsky_crypto::types::{Didable, ExportableKeypair};
use secp256k1::{Keypair, Secp256k1, SecretKey};
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use super::Repo;

pub struct ActorStoreConfig {
    pub directory: String,
    pub cache_size: usize,
    pub disable_wal_auto_checkpoint: bool,
}

impl ActorStoreConfig {
    pub fn new() -> Self {
        fn db_loc(loc_name: &str) -> String {
            let dir = env::var("PDS_DATA_DIRECTORY").unwrap_or("data".to_string());
            format!("{}/{}", dir, loc_name)
        }
        ActorStoreConfig {
            directory: env::var("PDS_ACTOR_STORE_DIRECTORY").unwrap_or(db_loc("actors")),
            cache_size: env::var("PDS_ACTOR_STORE_CACHE_SIZE")
                .unwrap_or("100".to_string())
                .parse::<usize>()
                .unwrap_or(100),
            disable_wal_auto_checkpoint: env::var("DISABLE_WAL_AUTO_CHECKPOINT")
                .unwrap_or("false".to_string())
                .parse::<bool>()
                .unwrap_or(false),
        }
    }
}

pub struct ActorStore {
    pub did: String,
    pub storage: SqlRepoReader, // get ipld blocks from db
    pub record: RecordReader,   // get lexicon records from db
    pub blob: BlobReader,       // get blobs
    pub pref: PreferenceReader, // get preferences
    reserved_key_dir: PathBuf,
}

// Combination of RepoReader/Transactor, BlobReader/Transactor, SqlRepoReader/Transactor
impl ActorStore {
    /// Concrete reader of an individual repo (hence S3BlobStore which takes `did` param)
    pub fn new(did: String, blobstore: S3BlobStore) -> Self {
        let cfg = ActorStoreConfig::new();
        ActorStore {
            storage: SqlRepoReader::new(None, did.clone(), None),
            record: RecordReader::new(did.clone()),
            pref: PreferenceReader::new(did.clone()),
            did,
            blob: BlobReader::new(blobstore), // Unlike TS impl, just use blob reader vs generator
            reserved_key_dir: Path::new(&cfg.directory).join("reserved_keys"),
        }
    }

    // Transactors
    // -------------------

    // @TODO: Update to use AtUri
    pub async fn create_repo(
        &mut self,
        keypair: Keypair,
        writes: Vec<PreparedCreateOrUpdate>,
    ) -> Result<CommitData> {
        let write_ops = writes
            .clone()
            .into_iter()
            .map(|prepare| {
                let uri_without_prefix = prepare.uri.replace("at://", "");
                let parts = uri_without_prefix.split("/").collect::<Vec<&str>>();
                let collection = *parts.get(0).unwrap_or(&"");
                let rkey = *parts.get(1).unwrap_or(&"");

                RecordCreateOrUpdateOp {
                    action: WriteOpAction::Create,
                    collection: collection.to_owned(),
                    rkey: rkey.to_owned(),
                    record: prepare.record,
                }
            })
            .collect::<Vec<RecordCreateOrUpdateOp>>();
        let commit = Repo::format_init_commit(
            self.storage.clone(),
            self.did.clone(),
            keypair,
            Some(write_ops),
        )?;
        self.storage.apply_commit(commit.clone(), None).await?;
        let writes = writes
            .into_iter()
            .map(|w| PreparedWrite::Create(w))
            .collect::<Vec<PreparedWrite>>();
        self.blob.process_write_blobs(writes).await?;
        Ok(commit)
    }

    pub async fn process_writes(
        &mut self,
        writes: Vec<PreparedWrite>,
        swap_commit_cid: Option<Cid>,
    ) -> Result<CommitData> {
        let commit = self.format_commit(writes.clone(), swap_commit_cid).await?;
        {
            let immutable_borrow = &self;
            // & send to indexing
            immutable_borrow
                .index_writes(writes.clone(), &commit.rev)
                .await?;
        }
        try_join!(
            // persist the commit to repo storage
            self.storage.apply_commit(commit.clone(), None),
            // process blobs
            self.blob.process_write_blobs(writes)
        )?;
        Ok(commit)
    }

    pub async fn format_commit(
        &mut self,
        writes: Vec<PreparedWrite>,
        swap_commit: Option<Cid>,
    ) -> Result<CommitData> {
        let current_root = self.storage.get_root_detailed().await;
        if let Ok(current_root) = current_root {
            if let Some(swap_commit) = swap_commit {
                if !current_root.cid.eq(&swap_commit) {
                    bail!("BadCommitSwapError: {0}", current_root.cid)
                }
            }
            self.storage.cache_rev(current_root.rev).await?;
            let mut new_record_cids: Vec<Cid> = vec![];
            let mut delete_and_update_uris: Vec<String> = vec![];
            for write in &writes {
                match write.clone() {
                    PreparedWrite::Create(c) => new_record_cids.push(c.cid),
                    PreparedWrite::Update(u) => {
                        new_record_cids.push(u.cid);
                        delete_and_update_uris.push(u.uri);
                    }
                    PreparedWrite::Delete(d) => delete_and_update_uris.push(d.uri),
                }
                if write.swap_cid().is_none() {
                    continue;
                }
                let record = self
                    .record
                    .get_record(write.uri(), None, Some(true))
                    .await?;
                let current_record = match record {
                    Some(record) => Some(Cid::from_str(&record.cid)?),
                    None => None,
                };
                match write {
                    // There should be no current record for a create
                    PreparedWrite::Create(_) if write.swap_cid().is_some() => {
                        bail!("BadRecordSwapError: `{0:?}`", current_record)
                    }
                    // There should be a current record for an update
                    PreparedWrite::Update(_) if write.swap_cid().is_none() => {
                        bail!("BadRecordSwapError: `{0:?}`", current_record)
                    }
                    // There should be a current record for a delete
                    PreparedWrite::Delete(_) if write.swap_cid().is_none() => {
                        bail!("BadRecordSwapError: `{0:?}`", current_record)
                    }
                    _ => Ok::<(), anyhow::Error>(()),
                }?;
                match (current_record, write.swap_cid()) {
                    (Some(current_record), Some(swap_cid)) if current_record.eq(swap_cid) => {
                        Ok::<(), anyhow::Error>(())
                    }
                    _ => bail!(
                        "BadRecordSwapError: current record is `{0:?}`",
                        current_record
                    ),
                }?;
            }
            let mut repo = Repo::load(&mut self.storage, Some(current_root.cid)).await?;
            let write_ops: Vec<RecordWriteOp> = writes
                .into_iter()
                .map(|write| write_to_op(write))
                .collect::<Vec<RecordWriteOp>>();
            // @TODO: Use repo signing key global config
            let secp = Secp256k1::new();
            let repo_private_key = env::var("PDS_REPO_SIGNING_KEY_K256_PRIVATE_KEY_HEX").unwrap();
            let repo_secret_key =
                SecretKey::from_slice(&hex::decode(repo_private_key.as_bytes()).unwrap()).unwrap();
            let repo_signing_key = Keypair::from_secret_key(&secp, &repo_secret_key);
            let mut commit = repo
                .format_commit(RecordWriteEnum::List(write_ops), repo_signing_key)
                .await?;

            // find blocks that would be deleted but are referenced by another record
            let duplicate_record_cids = self
                .get_duplicate_record_cids(commit.removed_cids.to_list(), delete_and_update_uris)
                .await?;
            for cid in duplicate_record_cids {
                commit.removed_cids.delete(cid)
            }

            // find blocks that are relevant to ops but not included in diff
            // (for instance a record that was moved but cid stayed the same)
            let new_record_blocks = commit.new_blocks.get_many(new_record_cids)?;
            if new_record_blocks.missing.len() > 0 {
                let missing_blocks = self.storage.get_blocks(new_record_blocks.missing).await?;
                commit.new_blocks.add_map(missing_blocks.blocks)?;
            }
            Ok(commit)
        } else {
            bail!("No repo root found for `{0}`", self.did)
        }
    }

    pub async fn index_writes(&self, writes: Vec<PreparedWrite>, rev: &String) -> Result<()> {
        let now: &str = &common::now();

        let _ = stream::iter(writes)
            .then(|write| async move {
                Ok::<(), anyhow::Error>(match write {
                    PreparedWrite::Create(write) => {
                        self.record
                            .index_record(
                                write.uri,
                                write.cid,
                                Some(write.record),
                                Some(write.action),
                                rev.clone(),
                                Some(now.to_string()),
                            )
                            .await?
                    }
                    PreparedWrite::Update(write) => {
                        self.record
                            .index_record(
                                write.uri,
                                write.cid,
                                Some(write.record),
                                Some(write.action),
                                rev.clone(),
                                Some(now.to_string()),
                            )
                            .await?
                    }
                    PreparedWrite::Delete(write) => self.record.delete_record(write.uri).await?,
                })
            })
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }

    pub async fn destroy(&mut self) -> Result<()> {
        use crate::schema::pds::blob::dsl as BlobSchema;
        let conn = &mut establish_connection()?;

        let blob_rows: Vec<String> = BlobSchema::blob
            .filter(BlobSchema::did.eq(&self.did))
            .select(BlobSchema::cid)
            .get_results(conn)?;
        let cids = blob_rows
            .into_iter()
            .map(|row| Ok(Cid::from_str(&row)?))
            .collect::<Result<Vec<Cid>>>()?;
        let _ = stream::iter(cids.chunks(500))
            .then(|chunk| async {
                Ok::<(), anyhow::Error>(self.blob.blobstore.delete_many(chunk.to_vec()).await?)
            })
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }

    // @TODO: Use AtUri
    pub async fn get_duplicate_record_cids(
        &self,
        cids: Vec<Cid>,
        touched_uris: Vec<String>,
    ) -> Result<Vec<Cid>> {
        if touched_uris.len() == 0 || cids.len() == 0 {
            return Ok(vec![]);
        }
        use crate::schema::pds::record::dsl as RecordSchema;
        let conn = &mut establish_connection()?;

        let cid_strs: Vec<String> = cids.into_iter().map(|c| c.to_string()).collect();
        let res: Vec<String> = RecordSchema::record
            .filter(RecordSchema::did.eq(&self.did))
            .filter(RecordSchema::cid.eq_any(cid_strs))
            .filter(RecordSchema::uri.ne_all(touched_uris))
            .select(RecordSchema::cid)
            .get_results(conn)?;
        Ok(res
            .into_iter()
            .map(|row| Cid::from_str(&row).map_err(|error| anyhow::Error::new(error)))
            .collect::<Result<Vec<Cid>>>()?)
    }

    pub async fn reserve_keypair(&self, did: Option<&str>) -> Result<String> {
        if let Some(did) = did {
            assert_safe_path_part(&did);
            let key_loc = Path::new(&self.reserved_key_dir).join(did);
            let key = load_key(key_loc);
            if key.is_ok() {
                return Ok(key?.did()?);
            }
        }
        let keypair = Secp256k1Keypair::create(Some(Secp256k1KeypairOptions {
            exportable: Some(true),
        }))
        .await?;
        let key_did = keypair.did()?;
        let key_loc = Path::new(&self.reserved_key_dir).join(&key_did);
        fs::create_dir_all(self.reserved_key_dir.clone())?;
        fs::write(key_loc, keypair.export()?)?;
        Ok(key_did)
    }
}

fn load_key(loc: PathBuf) -> Result<Secp256k1Keypair> {
    let priv_key = File::open(loc)?
        .bytes()
        .map(|b| b.unwrap())
        .collect::<Vec<u8>>();

    Ok(Secp256k1Keypair::import(
        priv_key.as_slice(),
        Some(Secp256k1KeypairOptions {
            exportable: Some(true),
        }),
    )?)
}

fn assert_safe_path_part(part: &str) {
    // TODO: need to replicate TS path.normalize
    // let normalized = &part;
    assert!(
        //   part == normalized &&
        part.as_bytes().get(0) != Some(&b'.') && !part.contains('/') && !part.contains('\\'),
        "unsafe path part: {}",
        part
    )
}
