use super::status;
use super::offline::commit_speaker_labels;
use sqlx::SqlitePool;

#[tokio::test]
async fn disabled_short_circuit_returns_zero() {
    super::update_status(super::DiarizationStatus {
        enabled: false,
        min_speakers: 2,
        max_speakers: 4,
        model_status: "disabled".to_string(),
    });
    // The disabled short-circuit returns before any DB call, but we must not
    // fabricate a `SqlitePool` via `mem::zeroed` (UB - aborts on Windows).
    // A real in-memory pool is cheap and safe.
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite pool must connect");
    let res = commit_speaker_labels(
        &pool,
        "meeting-disabled",
        None,
        Vec::new(),
        2,
        4,
    )
    .await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap_or(99), 0);
    pool.close().await;
    // Restore default for other tests.
    super::update_status(status());
}
