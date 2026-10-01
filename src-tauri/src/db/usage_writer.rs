use crate::{
    db::{repo::usage, Db},
    domain::usage::NewUsage,
};
use std::{
    sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

const QUEUE_CAPACITY: usize = 4096;
const MAX_BATCH: usize = 256;

enum Message {
    Record(NewUsage),
    Flush(oneshot::Sender<()>),
}

/// Persists per-request token usage off the request path.
///
/// Requests enqueue into a bounded channel; a dedicated thread drains it and writes
/// whatever accumulated in one transaction. The thread exits once every handle is dropped.
#[derive(Clone)]
pub struct UsageWriter {
    sender: SyncSender<Message>,
}

impl UsageWriter {
    pub fn spawn(db: Db) -> Self {
        let (sender, receiver) = sync_channel(QUEUE_CAPACITY);
        let spawned = std::thread::Builder::new()
            .name("usage-writer".into())
            .spawn(move || run(&db, &receiver));
        if let Err(error) = spawned {
            tracing::error!(%error, "usage writer thread could not start");
        }
        Self { sender }
    }

    pub fn record(&self, usage: NewUsage) {
        match self.sender.try_send(Message::Record(usage)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!("usage queue is full; dropping one usage record");
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::warn!("usage writer stopped; dropping one usage record");
            }
        }
    }

    /// Records token counts, ignoring empty responses.
    #[allow(clippy::too_many_arguments)]
    pub fn record_counts(
        &self,
        provider_name: &str,
        protocol: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cached_tokens: u32,
        reasoning_tokens: u32,
    ) {
        if input_tokens == 0 && output_tokens == 0 {
            return;
        }
        self.record(NewUsage {
            provider_name: provider_name.to_owned(),
            protocol: protocol.to_owned(),
            model: model.to_owned(),
            input_tokens,
            output_tokens,
            cached_tokens,
            reasoning_tokens,
            requested_at: now_seconds(),
        });
    }

    /// Resolves once everything enqueued before this call has been written.
    pub async fn flush(&self) {
        let (done, wait) = oneshot::channel();
        if self.sender.send(Message::Flush(done)).is_ok() {
            let _ = wait.await;
        }
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

fn run(db: &Db, receiver: &Receiver<Message>) {
    while let Ok(first) = receiver.recv() {
        let mut batch = Vec::new();
        let mut flushes = Vec::new();
        let mut next = Some(first);
        while let Some(message) = next.take() {
            match message {
                Message::Record(usage) => batch.push(usage),
                Message::Flush(done) => flushes.push(done),
            }
            if batch.len() < MAX_BATCH {
                next = receiver.try_recv().ok();
            }
        }
        if let Err(error) = write(db, &batch) {
            tracing::warn!(%error, dropped = batch.len(), "failed to persist request usage");
        }
        for done in flushes {
            let _ = done.send(());
        }
    }
}

fn write(db: &Db, batch: &[NewUsage]) -> crate::error::Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let mut conn = db.conn()?;
    usage::record_batch(&mut conn, batch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn batches_are_written_and_flush_waits_for_them() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("usage.db")).unwrap();
        let writer = UsageWriter::spawn(db.clone());
        for index in 0..600 {
            writer.record_counts("P", "responses", &format!("m{}", index % 3), 10, 5, 0, 0);
        }
        writer.record_counts("P", "responses", "ignored", 0, 0, 0, 0);
        writer.flush().await;
        let snapshot = usage::snapshot_for_days(&db.conn().unwrap(), 1, None).unwrap();
        assert_eq!(snapshot.total_requests, 600);
        assert_eq!(snapshot.models.len(), 3);
    }
}
