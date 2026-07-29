use std::sync::Arc;

use tokio::{sync::Notify, time::Duration};

use crate::{config::AppConfig, db::Database, printers};

#[derive(Clone)]
pub struct QueueSignal(Arc<Notify>);

impl QueueSignal {
    pub fn new() -> Self {
        Self(Arc::new(Notify::new()))
    }

    pub fn notify(&self) {
        self.0.notify_one();
    }

    async fn wait(&self) {
        self.0.notified().await;
    }
}

impl Default for QueueSignal {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn run(config: AppConfig, db: Database, signal: QueueSignal) {
    loop {
        match db.claim_next_job() {
            Ok(Some((job, path))) => {
                let worker_config = config.clone();
                let printer_id = job.printer_id.clone();
                let job_id = job.id.clone();
                let copies = job.copies;
                let result = tokio::task::spawn_blocking(move || {
                    printers::submit(&worker_config, &job_id, &printer_id, &path, copies)
                })
                .await;
                match result {
                    Ok(Ok(detail)) => {
                        if let Err(error) = db.finish_job(&job.id, "succeeded", &detail) {
                            tracing::error!(job_id = %job.id, error = %error, "could not persist job success");
                        }
                    }
                    Ok(Err(error)) => {
                        if let Err(db_error) =
                            db.finish_job(&job.id, "failed", &safe_error(&error.to_string()))
                        {
                            tracing::error!(job_id = %job.id, error = %db_error, "could not persist job failure");
                        }
                    }
                    Err(error) => {
                        if let Err(db_error) =
                            db.finish_job(&job.id, "failed", "Printer worker stopped unexpectedly")
                        {
                            tracing::error!(job_id = %job.id, error = %db_error, "could not persist worker failure");
                        }
                        tracing::error!(job_id = %job.id, error = %error, "printer task failed");
                    }
                }
            }
            Ok(None) => {
                tokio::select! {
                    () = signal.wait() => {}
                    () = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
            Err(error) => {
                tracing::error!(error = %error, "queue claim failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

fn safe_error(value: &str) -> String {
    let without_lines = value.lines().next().unwrap_or("Printer submission failed");
    without_lines.chars().take(300).collect()
}
