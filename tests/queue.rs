use std::sync::{Arc, Barrier};

use printlatch::{
    auth,
    config::AppConfig,
    db::{Database, NewJob},
};

struct QueueHarness {
    _temp: tempfile::TempDir,
    db: Database,
    client_id: String,
    pdf_path: String,
}

impl QueueHarness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temporary directory");
        let config =
            AppConfig::resolve(Some(temp.path().to_path_buf()), None).expect("test config");
        config.ensure_directories().expect("test directories");
        let db = Database::open(config.database_path()).expect("database");
        let client = auth::issue_local_token(&db, "queue test", 1).expect("client");
        let pdf_path = config.jobs_dir().join("fixture.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4\n%%EOF\n").expect("fixture");
        Self {
            _temp: temp,
            db,
            client_id: client.client_id,
            pdf_path: pdf_path.to_string_lossy().into_owned(),
        }
    }

    fn insert(&self, id: &str) {
        self.db
            .insert_job(&NewJob {
                id,
                client_id: &self.client_id,
                printer_id: "capture:pdf",
                state: "queued",
                mode: "print",
                copies: 1,
                page_count: 1,
                byte_count: 16,
                sha256: "fixture",
                file_path: &self.pdf_path,
            })
            .expect("insert job");
    }
}

#[test]
fn only_one_worker_can_claim_a_job() {
    let harness = QueueHarness::new();
    harness.insert("one");
    assert!(harness.db.claim_next_job().expect("first claim").is_some());
    assert!(harness.db.claim_next_job().expect("second claim").is_none());
}

#[test]
fn cancel_and_claim_race_has_one_winner() {
    let harness = QueueHarness::new();
    harness.insert("race");
    let barrier = Arc::new(Barrier::new(3));
    let cancel_db = harness.db.clone();
    let cancel_client = harness.client_id.clone();
    let cancel_barrier = barrier.clone();
    let cancel = std::thread::spawn(move || {
        cancel_barrier.wait();
        cancel_db
            .cancel_job("race", &cancel_client)
            .expect("cancel attempt")
    });
    let claim_db = harness.db.clone();
    let claim_barrier = barrier.clone();
    let claim = std::thread::spawn(move || {
        claim_barrier.wait();
        claim_db.claim_next_job().expect("claim attempt").is_some()
    });
    barrier.wait();
    let canceled = cancel.join().expect("cancel thread");
    let claimed = claim.join().expect("claim thread");
    assert_ne!(canceled, claimed, "exactly one transition must win");
    let (state, _) = harness
        .db
        .job_state("race", &harness.client_id)
        .expect("state query")
        .expect("job");
    assert!(matches!(state.as_str(), "canceled" | "printing"));
}

#[test]
fn restart_during_submission_becomes_unknown_not_queued() {
    let harness = QueueHarness::new();
    harness.insert("interrupted");
    assert!(harness.db.claim_next_job().expect("claim").is_some());
    assert_eq!(
        harness
            .db
            .recover_interrupted_jobs()
            .expect("recover interrupted"),
        1
    );
    let (state, _) = harness
        .db
        .job_state("interrupted", &harness.client_id)
        .expect("state query")
        .expect("job");
    assert_eq!(state, "unknown");
    assert!(
        harness
            .db
            .claim_next_job()
            .expect("post-restart claim")
            .is_none(),
        "the agent must not risk a duplicate print after restart"
    );
}

#[test]
fn retries_are_explicit_and_capped_at_three_attempts() {
    let harness = QueueHarness::new();
    harness.insert("retry");
    for attempt in 1..=3 {
        let (job, _) = harness
            .db
            .claim_next_job()
            .expect("claim")
            .expect("queued job");
        assert_eq!(job.attempts, attempt);
        harness
            .db
            .finish_job("retry", "failed", "simulated printer failure")
            .expect("fail job");
        let queued = harness
            .db
            .retry_job("retry", &harness.client_id)
            .expect("retry request");
        assert_eq!(queued, attempt < 3);
    }
}
