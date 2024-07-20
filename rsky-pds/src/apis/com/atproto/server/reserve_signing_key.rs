use aws_config::SdkConfig;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{response::status, State};

use crate::models::{InternalErrorCode, InternalErrorMessageResponse};
use crate::repo::aws::s3::S3BlobStore;
use crate::repo::ActorStore;
use rsky_lexicon::com::atproto::server::{ReserveSigningKeyInput, ReserveSigningKeyOutput};

#[rocket::post(
    "/xrpc/com.atproto.server.reserveSigningKey",
    format = "json",
    data = "<body>"
)]
pub async fn reserve_signing_key(
    body: Json<ReserveSigningKeyInput>,
    s3_config: &State<SdkConfig>,
) -> Result<Json<ReserveSigningKeyOutput>, status::Custom<Json<InternalErrorMessageResponse>>> {
    let did = &body.did;
    let actor_store = ActorStore::new(did.clone(), S3BlobStore::new(did.clone(), s3_config));
    match actor_store.reserve_keypair(Some(did)).await {
        Ok(key) => Ok(Json(ReserveSigningKeyOutput { body: key })),
        Err(error) => {
            let internal_error = InternalErrorMessageResponse {
                code: Some(InternalErrorCode::InternalError),
                message: Some(error.to_string()),
            };
            Err(status::Custom(
                Status::InternalServerError,
                Json(internal_error),
            ))
        }
    }
}
