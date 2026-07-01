//! Query behavior: present keys resolve, absent keys miss, string keys work.

mod common;

use pachash::{EliasFanoIndex, PaCHashObjectStore};
use std::collections::HashSet;

#[test]
fn present_and_absent_keys() {
    let items = common::gen_items(200, 300);
    let present: HashSet<u64> = items.iter().map(|(k, _)| *k).collect();
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items.clone()).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    for (key, value) in &items {
        assert_eq!(store.query(*key).unwrap().as_ref(), &value[..]);
    }

    // Keys just outside the present set must miss.
    let mut misses = 0;
    for probe in 1_000_000u64..1_000_500 {
        if !present.contains(&probe) {
            assert!(store.query(probe).is_none());
            misses += 1;
        }
    }
    assert!(misses > 0);
}

#[test]
fn absent_key_returns_none_not_neighbor() {
    // A miss must not return a nearby key's value.
    let items = vec![
        (100u64, b"hundred".to_vec()),
        (200u64, b"two hundred".to_vec()),
        (300u64, b"three hundred".to_vec()),
    ];
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    assert_eq!(store.query(100).unwrap().as_ref(), b"hundred");
    assert!(store.query(150).is_none());
    assert!(store.query(250).is_none());
    assert!(store.query(99).is_none());
    assert!(store.query(301).is_none());
}

#[test]
fn string_api_roundtrip() {
    let items: Vec<(String, String)> = (0..50)
        .map(|i| (format!("key_{i}"), format!("value number {i}")))
        .collect();
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file_strings(&items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    for (k, v) in &items {
        let result = store.query_string(k).expect("string key present");
        assert_eq!(result.as_ref(), v.as_bytes());
    }
    assert!(store.query_string("key_that_is_absent").is_none());
}

#[test]
fn query_on_empty_store_misses() {
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(vec![]).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();
    assert_eq!(store.num_objects(), 0);
    for key in [1u64, 42, u64::MAX] {
        assert!(store.query(key).is_none());
    }
}

#[test]
fn display_names_multiplier_and_index() {
    let bytes =
        PaCHashObjectStore::<EliasFanoIndex>::write_to_file(vec![(1u64, vec![0u8; 10])]).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();
    assert_eq!(store.to_string(), "PaCHashObjectStore a=8 index=EliasFano");
}
