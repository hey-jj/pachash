//! Public API surface that the other suites do not touch.
//!
//! Covers the index name strings, the store name format for each index, the
//! query buffer and IO formulas, the store accessors, and the non-overlapping
//! table lookup used by the separator and cuckoo layouts.

mod common;

use pachash::{
    BlockStorage, CompressedBitVectorIndex, EliasFanoIndex, Index, PaCHashObjectStore,
    UncompressedBitVectorIndex, BLOCK_LENGTH,
};

#[test]
fn index_names_are_stable() {
    // These strings feed the store name and any diagnostic output. A change here
    // would break compatibility with files or logs that record the index kind.
    assert_eq!(EliasFanoIndex::name(), "EliasFano");
    assert_eq!(UncompressedBitVectorIndex::name(), "UncompressedBitVector");
    assert_eq!(CompressedBitVectorIndex::name(), "CompressedBitVector");
}

#[test]
fn store_name_uses_index_name() {
    assert_eq!(
        PaCHashObjectStore::<EliasFanoIndex>::name(8),
        "PaCHashObjectStore a=8 indexStructure=EliasFano"
    );
    assert_eq!(
        PaCHashObjectStore::<UncompressedBitVectorIndex>::name(1),
        "PaCHashObjectStore a=1 indexStructure=UncompressedBitVector"
    );
    assert_eq!(
        PaCHashObjectStore::<CompressedBitVectorIndex>::name(16),
        "PaCHashObjectStore a=16 indexStructure=CompressedBitVector"
    );
}

#[test]
fn buffer_and_io_formulas() {
    // required_buffer_per_query = 4 * (max_size + BLOCK_LENGTH - 1). required IOs
    // is always 1 for PaCHash.
    let items = vec![(1u64, vec![0u8; 5000]), (2u64, vec![7u8; 100])];
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    assert_eq!(store.max_size(), 5000);
    assert_eq!(
        store.required_buffer_per_query(),
        4 * (5000 + BLOCK_LENGTH - 1)
    );
    assert_eq!(store.required_ios_per_query(), 1);
}

#[test]
fn store_accessors_report_shape() {
    let items = common::gen_items(20, 300);
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    assert_eq!(store.a(), 8);
    assert_eq!(store.num_objects(), 20);
    assert!(store.num_blocks() >= 1);
    assert_eq!(store.data().len(), store.num_blocks() * BLOCK_LENGTH);
    // The index reports a nonzero backing size once blocks exist.
    assert!(store.index_space() > 0);
}

#[test]
fn key2bin_method_matches_free_function() {
    let items = common::gen_items(10, 100);
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

    let num_bins = (store.num_blocks() * 8) as u64;
    for key in [0u64, 1, 12345, 1 << 40, u64::MAX] {
        assert_eq!(
            store.key2bin(key) as u64,
            pachash::key2bin(key, num_bins),
            "key {key}"
        );
    }
}

#[test]
fn find_key_non_overlapping_shifted_offsets() {
    // The separator and cuckoo writers store shifted cumulative offsets. Object 0
    // starts at 0. Object i > 0 starts at offsets[i - 1]. The last entry holds the
    // end of the last object.
    let mut data = vec![0u8; BLOCK_LENGTH];
    data[0..3].copy_from_slice(b"abc");
    data[3..8].copy_from_slice(b"defgh");
    // Shifted table: offsets = [3, 8] means object 0 is [0,3), object 1 is [3,8).
    BlockStorage::write_table(&mut data, 2, 0, &[10u64, 20u64], &[3u16, 8u16]);
    let block = BlockStorage::parse(&data);

    assert_eq!(block.find_key_non_overlapping(10), Some((3, 0)));
    assert_eq!(block.find_key_non_overlapping(20), Some((5, 3)));
    assert_eq!(block.find_key_non_overlapping(99), None);
}
