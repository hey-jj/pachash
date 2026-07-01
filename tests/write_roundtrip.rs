//! Round-trip: write a store, read every object back, compare to the input.

mod common;

use pachash::{EliasFanoIndex, LinearObjectReader, PaCHashObjectStore};
use std::collections::HashMap;

/// Build a store, read it back with the linear reader, and check the object set
/// matches the input regardless of order.
fn roundtrip(items: Vec<(u64, Vec<u8>)>) {
    let expected: HashMap<u64, Vec<u8>> = items.iter().cloned().collect();
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();

    let reader = LinearObjectReader::new(&bytes).unwrap();
    let objects = reader.read_all();

    assert_eq!(objects.len(), expected.len(), "object count");
    let mut seen = HashMap::new();
    for obj in objects {
        assert!(obj.key != 0, "no terminator or header leaks through");
        let want = expected.get(&obj.key).expect("unknown key read back");
        assert_eq!(&obj.value, want, "value for key {}", obj.key);
        assert!(
            seen.insert(obj.key, ()).is_none(),
            "duplicate key read back"
        );
    }
}

#[test]
fn roundtrip_small_values() {
    roundtrip(common::gen_items(100, 40));
}

#[test]
fn roundtrip_medium_values() {
    roundtrip(common::gen_items(80, 800));
}

#[test]
fn roundtrip_block_filling_values() {
    // Sizes near a full data block exercise the exact-fill boundaries.
    roundtrip(common::gen_items(60, 4086));
}

#[test]
fn roundtrip_multi_block_values() {
    roundtrip(common::gen_items(40, 20000));
}

#[test]
fn roundtrip_single_object() {
    roundtrip(vec![(7u64, b"only one".to_vec())]);
}

#[test]
fn roundtrip_empty_store() {
    roundtrip(vec![]);
}

#[test]
fn roundtrip_mixed_sizes() {
    let mut items = Vec::new();
    for i in 0..50u64 {
        let len = match i % 5 {
            0 => 0,
            1 => 10,
            2 => 4000,
            3 => 9000,
            _ => 30000,
        };
        items.push((i + 1, vec![(i & 0xFF) as u8; len]));
    }
    roundtrip(items);
}

#[test]
fn query_finds_every_key() {
    let items = common::gen_items(120, 600);
    let expected: HashMap<u64, Vec<u8>> = items.iter().cloned().collect();
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    assert_eq!(store.num_objects(), expected.len());
    for (key, value) in &expected {
        let result = store.query(*key).expect("key present");
        assert_eq!(result.as_ref(), &value[..], "key {key}");
    }
}
