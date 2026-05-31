//! Background worker that drains the task queue.

use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::executor::{run_task, ExecCtx};
use crate::model::TaskStatus;

/// Handle to a spawned worker. Drop or `.shutdown()` to stop it after the
/// current task finishes.
pub struct WorkerHandle {
    stop:   Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

impl WorkerHandle {
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        let _ = self.handle.await;
    }
}

/// Spawn the background worker.
///
/// Polling cadence is intentionally simple — every `poll_interval` ms we
/// claim the next pending task and run it. For dev/small panels this is
/// fine; a high-throughput deployment would replace this with notify-driven
/// dispatch (LISTEN/NOTIFY on PG, a channel on SQLite).
pub fn spawn_worker(ctx: ExecCtx, poll_interval: Duration) -> WorkerHandle {
    let (stop_tx, mut stop_rx) = oneshot::channel();

    // Best-effort orphan recovery on startup.
    let recover_ctx = ctx.clone();
    let handle = tokio::spawn(async move {
        match recover_ctx.tasks.recover_orphans().await {
            Ok(0) => {}
            Ok(n) => tracing::warn!("recovered {} orphaned task(s) from previous run", n),
            Err(e) => tracing::warn!(%e, "failed to recover orphan tasks"),
        }

        tracing::info!("task worker started (poll = {:?})", poll_interval);

        loop {
            tokio::select! {
                _ = &mut stop_rx => {
                    tracing::info!("task worker stopping");
                    return;
                }
                _ = tokio::time::sleep(poll_interval) => {}
            }

            // Claim and run a task. Errors here are about queue plumbing,
            // not task execution — log and keep going.
            let task = match recover_ctx.tasks.claim_next().await {
                Ok(Some(t)) => t,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(%e, "claim_next failed");
                    continue;
                }
            };

            tracing::info!(task_id = task.id, kind = task.kind.as_str(), node_id = task.node_id, "task start");

            match run_task(&recover_ctx, &task).await {
                Ok(()) => {
                    let _ = recover_ctx.tasks.finish(task.id, TaskStatus::Success, None).await;
                    tracing::info!(task_id = task.id, "task success");
                }
                Err(err) => {
                    tracing::warn!(task_id = task.id, %err, "task failed");
                    let _ = recover_ctx
                        .tasks
                        .append_log(task.id, &format!("[error] {err}"))
                        .await;
                    let _ = recover_ctx
                        .tasks
                        .finish(task.id, TaskStatus::Failed, Some(&err))
                        .await;
                }
            }
        }
    });

    WorkerHandle {
        stop: Some(stop_tx),
        handle,
    }
}
