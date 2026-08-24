//! NME-SC lite: cosine-affinity spectral clustering in pure Rust.
//!
//! Used by the offline diarization pass (PR-44b). The implementation is a
//! deliberately small port — VBx-style Bayesian refinement is out of scope
//! for v1 — but it ships deterministic labels for 2-6 speakers without any
//! external dependency beyond `nalgebra`.
//!
//! Reference: Landini et al., "Bayesian HMM clustering of x-vector
//! sequences" (VBx). We approximate with a normalised-cut eigengap heuristic.

use nalgebra::{DMatrix, DVector};
use std::collections::HashMap;

/// Cosine affinity matrix; clips negatives to 0 (binary NME-SC lite).
pub fn cosine_affinity(emb: &[Vec<f32>]) -> DMatrix<f32> {
    let n = emb.len();
    let norms: Vec<f32> = emb
        .iter()
        .map(|v| (v.iter().map(|x| x * x).sum::<f32>()).sqrt())
        .collect();
    let mut m = DMatrix::<f32>::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            if i == j {
                m[(i, j)] = 1.0;
                continue;
            }
            if norms[i] == 0.0 || norms[j] == 0.0 {
                continue;
            }
            let dot: f32 = emb[i].iter().zip(emb[j].iter()).map(|(a, b)| a * b).sum();
            m[(i, j)] = (dot / (norms[i] * norms[j])).max(0.0);
        }
    }
    m
}

/// Spectral cluster embeddings into `k` speakers. Returns raw cluster ids
/// (0..k-1) in input order.
pub fn spectral_cluster(emb: &[Vec<f32>], k: usize) -> Vec<usize> {
    let n = emb.len();
    if n == 0 {
        return Vec::new();
    }
    if n <= k {
        return (0..n).map(|i| i.min(k - 1)).collect();
    }
    let w = cosine_affinity(emb);
    let deg: DVector<f32> = DVector::from_iterator(w.nrows(), w.row_iter().map(|r| r.sum()));
    let mut l = DMatrix::<f32>::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            l[(i, j)] = if i == j { deg[i] } else { 0.0 } - w[(i, j)];
        }
    }
    let eig = l.symmetric_eigen();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| {
        eig.eigenvalues[a]
            .partial_cmp(&eig.eigenvalues[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut feats = DMatrix::<f32>::zeros(n, k);
    for (new_i, &old_i) in idx.iter().take(k).enumerate() {
        for r in 0..n {
            feats[(r, new_i)] = eig.eigenvectors[(r, old_i)];
        }
    }
    kmeans(&feats, k)
}

fn kmeans(features: &DMatrix<f32>, k: usize) -> Vec<usize> {
    let n = features.nrows();
    let dims = features.ncols();

    // Deterministic farthest-point initialisation. Seeding with the first k
    // rows is degenerate when those embeddings are identical (e.g. the same
    // speaker opening a meeting): all centres collapse onto one point and
    // k-means can never separate the speakers.
    let mut centers = DMatrix::<f32>::zeros(k, dims);
    let mut chosen = vec![false; n];
    let mut best_dist = vec![f32::MAX; n];
    let mut pick = 0usize;
    for c in 0..k {
        chosen[pick] = true;
        for d in 0..dims {
            centers[(c, d)] = features[(pick, d)];
        }
        let mut far = 0usize;
        let mut far_d = -1.0_f32;
        for i in 0..n {
            if chosen[i] {
                continue;
            }
            let d: f32 = (0..dims)
                .map(|dd| {
                    let diff = features[(i, dd)] - centers[(c, dd)];
                    diff * diff
                })
                .sum();
            if d < best_dist[i] {
                best_dist[i] = d;
            }
            if best_dist[i] > far_d {
                far_d = best_dist[i];
                far = i;
            }
        }
        pick = far;
    }

    let mut labels = vec![0usize; n];
    for _ in 0..20 {
        for i in 0..n {
            let mut best = 0usize;
            let mut best_d = f32::MAX;
            for c in 0..k {
                let d: f32 = (0..features.ncols())
                    .map(|d| {
                        let diff = features[(i, d)] - centers[(c, d)];
                        diff * diff
                    })
                    .sum();
                if d < best_d {
                    best_d = d;
                    best = c;
                }
            }
            labels[i] = best;
        }
        let mut sums = DMatrix::<f32>::zeros(k, features.ncols());
        let mut counts = vec![0usize; k];
        for i in 0..n {
            let c = labels[i];
            counts[c] += 1;
            for d in 0..features.ncols() {
                sums[(c, d)] += features[(i, d)];
            }
        }
        for c in 0..k {
            if counts[c] > 0 {
                for d in 0..features.ncols() {
                    centers[(c, d)] = sums[(c, d)] / counts[c] as f32;
                }
            }
        }
    }
    labels
}

/// Renumber cluster ids by order of first appearance (1-indexed later).
pub fn remap_by_first_appearance(labels: &[usize]) -> Vec<usize> {
    let mut map = HashMap::new();
    let mut next = 0usize;
    labels
        .iter()
        .map(|l| {
            *map.entry(*l).or_insert_with(|| {
                let v = next;
                next += 1;
                v
            })
        })
        .collect()
}
