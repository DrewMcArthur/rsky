use rocket::futures::SinkExt;
use rocket_ws::{Channel, Message, WebSocket};

use crate::models::RepoSeq;

#[rocket::get("/xrpc/com.atproto.sync.subscribeRepos")]
pub async fn subscribe_repos<'a>(
    socket: WebSocket,
    sequencer: &'a rocket::State<crate::sequencer::Sequencer>,
) -> Channel<'a> {
    let mut rx = sequencer.subscribers.subscribe();

    // @TODO: Send backfilled messages
    // i think based on parameters in the original request,
    // we fetch backfill records from the DB?
    let records: Vec<RepoSeq> = vec![];

    socket.channel(move |mut stream| {
        Box::pin(async move {
            // @TODO: Send backfilled messages
            for record in records {
                let message =
                    serde_ipld_dagcbor::to_vec(&record).expect("Failed to serialize record");
                stream.send(Message::Binary(message)).await?;
            }

            // Send new messages
            while let Ok(record) = rx.recv().await {
                let message =
                    serde_ipld_dagcbor::to_vec(&record).expect("Failed to serialize record");
                if stream.send(Message::Binary(message)).await.is_err() {
                    break;
                }
            }

            Ok(())
        })
    })
}
