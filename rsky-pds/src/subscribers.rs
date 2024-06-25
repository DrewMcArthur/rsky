use anyhow::Result;
use rocket::tokio::sync::broadcast::{channel, Receiver, Sender};

use crate::models::RepoSeq;

#[derive(Debug)]
pub struct Subscribers {
    pub inner: Sender<RepoSeq>,
}

impl Subscribers {
    pub fn new() -> Self {
        let (tx, _) = channel(500);
        Self { inner: tx }
    }

    pub fn notify(&self, evt: RepoSeq) -> Result<()> {
        self.inner.send(evt)?;
        Ok(())
    }

    pub fn subscribe(&self) -> Receiver<RepoSeq> {
        self.inner.subscribe()
    }
}
