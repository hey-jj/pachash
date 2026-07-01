//! k-way merge of sorted stores.

mod common;

use pachash::{merge, EliasFanoIndex, MergeError, PaCHashObjectStore};
use std::collections::HashMap;

/// Build a store from integer items.
fn build_bytes(items: Vec<(u64, Vec<u8>)>) -> Vec<u8> {
    PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap()
}

#[test]
fn merge_two_disjoint_stores() {
    let a: Vec<(u64, Vec<u8>)> = (0..40u64).map(|i| (i * 2 + 1, vec![1u8; 100])).collect();
    let b: Vec<(u64, Vec<u8>)> = (0..40u64).map(|i| (i * 2 + 2, vec![2u8; 100])).collect();
    let expected: HashMap<u64, Vec<u8>> = a.iter().chain(b.iter()).cloned().collect();

    let merged = merge(&[build_bytes(a), build_bytes(b)]).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, merged).unwrap();

    assert_eq!(store.num_objects(), expected.len());
    for (key, value) in &expected {
        assert_eq!(&store.query(*key).unwrap().value, value, "key {key}");
    }
}

#[test]
fn merge_three_stores_with_overlap_objects() {
    let mut inputs = Vec::new();
    let mut expected: HashMap<u64, Vec<u8>> = HashMap::new();
    for group in 0..3u64 {
        let items: Vec<(u64, Vec<u8>)> = (0..15u64)
            .map(|i| {
                let key = group * 1000 + i + 1;
                let len = if i % 4 == 0 { 9000 } else { 60 };
                let value = vec![(group as u8) * 40 + i as u8; len];
                expected.insert(key, value.clone());
                (key, value)
            })
            .collect();
        inputs.push(build_bytes(items));
    }

    let merged = merge(&inputs).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, merged).unwrap();
    assert_eq!(store.num_objects(), expected.len());
    for (key, value) in &expected {
        assert_eq!(&store.query(*key).unwrap().value, value, "key {key}");
    }
}

#[test]
fn merge_result_is_valid_store() {
    let a = build_bytes(common::gen_items(30, 200));
    // Second store uses a disjoint key range by shifting keys.
    let b_items: Vec<(u64, Vec<u8>)> = common::gen_items(30, 200)
        .into_iter()
        .map(|(k, v)| (k + 1_000_000, v))
        .collect();
    let merged = merge(&[a, build_bytes(b_items)]).unwrap();

    // Building the index confirms the merged file is a well-formed PaCHash store.
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, merged).unwrap();
    assert_eq!(store.num_objects(), 60);
}

#[test]
fn merge_single_store_is_identity_on_contents() {
    let items = common::gen_items(25, 300);
    let expected: HashMap<u64, Vec<u8>> = items.iter().cloned().collect();
    let merged = merge(&[build_bytes(items)]).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, merged).unwrap();
    assert_eq!(store.num_objects(), expected.len());
    for (key, value) in &expected {
        assert_eq!(&store.query(*key).unwrap().value, value);
    }
}

#[test]
fn merge_detects_key_collision() {
    let shared: Vec<(u64, Vec<u8>)> = vec![(42u64, vec![1u8; 50])];
    let also_shared: Vec<(u64, Vec<u8>)> = vec![(42u64, vec![2u8; 50])];
    let err = merge(&[build_bytes(shared), build_bytes(also_shared)]).unwrap_err();
    assert_eq!(err, MergeError::KeyCollision(42));
}
