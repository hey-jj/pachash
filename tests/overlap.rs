//! Cross-block object reconstruction.
//!
//! An object larger than one block spills onto following blocks. A query must
//! stitch the fragments and drop the table bytes between them. This covers both
//! the "next block has objects" end case and the "fully overlapped" middle case.

mod common;

use pachash::{EliasFanoIndex, LinearObjectReader, PaCHashObjectStore, BLOCK_LENGTH};

#[test]
fn single_object_spanning_many_blocks() {
    // One object several blocks long, surrounded by small ones.
    let big = vec![0xABu8; 5 * BLOCK_LENGTH];
    let items = vec![
        (1u64, b"small before".to_vec()),
        (2u64, big.clone()),
        (3u64, b"small after".to_vec()),
    ];
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    assert_eq!(store.query(1).unwrap().as_ref(), b"small before");
    assert_eq!(store.query(2).unwrap().as_ref(), &big[..]);
    assert_eq!(store.query(3).unwrap().as_ref(), b"small after");
}

#[test]
fn object_exactly_one_block_of_data() {
    // Value sized so its data plus its table entry lands on a block edge.
    for extra in 0..40usize {
        let value = vec![0x5Au8; BLOCK_LENGTH - 100 + extra];
        let items = vec![(9u64, value.clone())];
        let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
        let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();
        assert_eq!(
            store.query(9).unwrap().as_ref(),
            &value[..],
            "extra={extra}"
        );
    }
}

#[test]
fn many_large_objects_reconstruct() {
    // Every object spans two to three blocks. Distinct byte fills catch stitching
    // errors that would splice the wrong fragment.
    let mut items = Vec::new();
    for i in 0..20u64 {
        let len = 2 * BLOCK_LENGTH + (i as usize) * 137;
        items.push((i + 1, vec![(i as u8).wrapping_add(1); len]));
    }
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items.clone()).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();
    for (key, value) in &items {
        assert_eq!(store.query(*key).unwrap().as_ref(), &value[..], "key {key}");
    }
}

#[test]
fn reader_and_query_agree_on_overlap() {
    let mut items = Vec::new();
    for i in 0..15u64 {
        let len = if i % 3 == 0 { 4 * BLOCK_LENGTH } else { 50 };
        items.push((i + 1, vec![(i as u8).wrapping_mul(3); len]));
    }
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items.clone()).unwrap();

    let reader = LinearObjectReader::new(&bytes).unwrap();
    for obj in reader.read_all() {
        let expected = &items.iter().find(|(k, _)| *k == obj.key).unwrap().1;
        assert_eq!(&obj.value, expected, "reader key {}", obj.key);
    }

    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();
    for (key, value) in &items {
        assert_eq!(
            store.query(*key).unwrap().as_ref(),
            &value[..],
            "query key {key}"
        );
    }
}
