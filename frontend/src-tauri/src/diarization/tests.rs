use super::*;

#[test]
fn buffer_push_and_drain() {
    let b = EmbeddingBuffer::default();
    b.push(WindowedEmbedding {
        audio_start: 0.0,
        audio_end: 1.0,
        vec: vec![0.0; EMBEDDING_DIM],
    });
    b.push(WindowedEmbedding {
        audio_start: 1.0,
        audio_end: 2.0,
        vec: vec![0.5; EMBEDDING_DIM],
    });
    let drained = b.drain();
    assert_eq!(drained.len(), 2);
    assert!(b.drain().is_empty());
}

#[test]
fn buffer_overflow_pops_oldest() {
    let b = EmbeddingBuffer::default();
    for i in 0..(MAX_BUFFER_WINDOWS + 5) {
        b.push(WindowedEmbedding {
            audio_start: i as f64,
            audio_end: i as f64 + 1.0,
            vec: vec![0.0; EMBEDDING_DIM],
        });
    }
    let drained = b.drain();
    assert_eq!(drained.len(), MAX_BUFFER_WINDOWS);
    assert_eq!(drained.first().unwrap().audio_start, 5.0);
}

#[test]
fn buffer_snapshot_is_clone_safe() {
    let b = EmbeddingBuffer::default();
    b.push(WindowedEmbedding {
        audio_start: 0.0,
        audio_end: 0.5,
        vec: vec![1.0; EMBEDDING_DIM],
    });
    let s = b.snapshot();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].audio_end, 0.5);
    // Snapshot must not drain the buffer.
    assert_eq!(b.len(), 1);
}
