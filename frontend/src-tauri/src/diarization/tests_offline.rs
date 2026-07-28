use super::status;
use super::offline::commit_speaker_labels;

#[tokio::test]
async fn disabled_short_circuit_returns_zero() {
    super::update_status(super::DiarizationStatus {
        enabled: false,
        min_speakers: 2,
        max_speakers: 4,
        model_status: "disabled".to_string(),
    });
    let res = commit_speaker_labels(
        // Pool is unused because we short-circuit before any DB call.
        &unsafe { std::mem::zeroed() },
        "meeting-disabled",
        None,
        Vec::new(),
        2,
        4,
    )
    .await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap_or(99), 0);
    // Restore default for other tests.
    super::update_status(status());
}
