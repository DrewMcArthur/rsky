use crate::repo::blob_refs::BlobRef;
use crate::{
    auth_verifier::AccessStandardIncludeChecks,
    common::tid::TID,
    repo::{aws::s3::S3BlobStore, ActorStore},
};
use anyhow::Result;
use aws_config::SdkConfig;
use bytes::Bytes;
use futures::Stream;
use libipld::{Cid, Multihash};
use rocket::serde::json::Json;
use rocket::State;
use rsky_lexicon::com::atproto::repo::ImportRepoInput;

#[rocket::post("/xrpc/com.atproto.repo.importRepo")]
pub async fn import_repo(
    body: Json<ImportRepoInput>,
    auth: AccessStandardIncludeChecks,
    s3_config: &State<SdkConfig>,
) {
    let did;
    let mut actor_store = ActorStore::new(did.clone(), S3BlobStore::new(did.clone(), s3_config));
    inner_import_repo(actor_store, body.into_inner()).await
}

async fn inner_import_repo(
    actor_store: ActorStore,
    incoming_car: impl Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let rev = TID::next_str();
    let did = actor_store.repo.did.clone();

    let (roots, blocks) = read_car_stream(incoming_car).await?;
    if roots.len() != 1 {
        return Err(InvalidRequestError::new("expected one root"));
    }

    let curr_root = actor_store
        .lock()
        .await
        .db
        .select_from("repo_root")
        .select_all()
        .execute_take_first()
        .await?;
    let curr_repo = if let Some(curr_root) = curr_root {
        Some(
            Repo::load(
                &actor_store.lock().await.repo.storage,
                Cid::try_from(curr_root.cid)?,
            )
            .await?,
        )
    } else {
        None
    };

    let diff = verify_diff(
        curr_repo.as_ref(),
        &blocks,
        &roots[0],
        None,
        None,
        EnsureLeaves::False,
    )
    .await?;
    diff.commit.rev = rev.clone();
    actor_store
        .lock()
        .await
        .repo
        .storage
        .apply_commit(&diff.commit, curr_repo.is_none())
        .await?;

    let record_queue = PQueue::new(50);
    let controller = AbortController::new();
    for write in diff.writes {
        record_queue
            .add(
                async move {
                    let uri = AtUri::make(&did, &write.collection, &write.rkey);
                    if write.action == WriteOpAction::Delete {
                        actor_store.lock().await.record.delete_record(&uri).await?;
                    } else {
                        let parsed_record = match get_and_parse_record(&blocks, &write.cid).await {
                            Ok(parsed) => parsed.record,
                            Err(_) => {
                                return Err(InvalidRequestError::new(&format!(
                                    "Could not parse record at '{}/{}'",
                                    write.collection, write.rkey
                                )))
                            }
                        };
                        let index_record = actor_store
                            .lock()
                            .await
                            .record
                            .index_record(
                                &uri,
                                &write.cid,
                                &parsed_record,
                                write.action,
                                &rev,
                                &now,
                            )
                            .await?;
                        let record_blobs = find_blob_refs(&parsed_record, 0);
                        let blob_values = record_blobs
                            .iter()
                            .map(|cid| RecordBlob {
                                record_uri: uri.to_string(),
                                blob_cid: cid,
                            })
                            .collect::<Vec<_>>();
                        if !blob_values.is_empty() {
                            actor_store
                                .lock()
                                .await
                                .db
                                .insert_into("record_blob")
                                .values(&blob_values)
                                .on_conflict_do_nothing()
                                .execute()
                                .await?;
                        }
                    }
                    Ok(())
                },
                controller.signal(),
            )
            .await?;
    }
    record_queue.on_idle().await;
    controller.signal().throw_if_aborted()?;
    Ok(())
}

pub fn find_blob_refs(val: &LexValue, layer: usize) -> Vec<BlobRef> {
    if layer > 32 {
        return vec![];
    }
    if let Some(array) = val.as_array() {
        return array
            .iter()
            .flat_map(|item| find_blob_refs(item, layer + 1))
            .collect();
    }
    if let Some(object) = val.as_object() {
        if let Some(blob_ref) = object.get::<BlobRef>("blob_ref") {
            return vec![blob_ref.clone()];
        }
        if object.get::<Cid>("cid").is_some() || object.get::<Multihash>("multihash").is_some() {
            return vec![];
        }
        return object
            .values()
            .flat_map(|item| find_blob_refs(item, layer + 1))
            .collect();
    }
    vec![]
}
