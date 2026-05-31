//! Persistent task queue + background worker.
//!
//! Operations against nodes (`apply_config` / `restart` / `check_health`)
//! are not executed inline in HTTP handlers — they're enqueued as rows in
//! `node_operation_tasks` and picked up by a background tokio task. That gives
//! us:
//!   - durability (a panel restart mid-deploy doesn't lose the task)
//!   - observability (UI can poll the row to stream the log)
//!   - safety (retries / rate-limiting / serialisation live in one place)
//!
//! Worker startup: any row stuck in `running` (left over from a previous
//! crash) is moved back to `pending`. From there the worker polls at a
//! short interval and runs one task at a time per node.

mod executor;
mod model;
mod repo;
mod worker;

pub use executor::{run_task, Adapters, ExecCtx};
pub use model::{NewTask, Task, TaskKind, TaskStatus};
pub use repo::TaskRepo;
pub use worker::{spawn_worker, WorkerHandle};
