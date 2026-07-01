//! Public API surface that the other suites do not touch.
//!
//! Covers the index name strings, the store Display label for each index, the
//! query buffer and IO formulas, the store accessors, and the writer's block
//! counter.

mod common;

use pachash::{
    CompressedBitVectorIndex, EliasFanoIndex, Index, LinearObjectWriter, PaCHashObjectStore,
    StoreMetadata, UncompressedBitVectorIndex, BLOCK_LENGTH,
};

#[test]
fn index_names_are_stable() {
    // These strings feed the store label and any diagnostic output. A change
    // here would break logs that record the index kind.
    assert_eq!(EliasFanoIndex::name(), "EliasFano");
    assert_eq!(UncompressedBitVectorIndex::name(), "UncompressedBitVector");
    assert_eq!(CompressedBitVectorIndex::name(), "CompressedBitVector");
}

#[test]
fn store_display_uses_index_name() {
    let make = |a: u16| {
        let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(vec![(1u64, vec![0u8; 8])])
            .unwrap();
        PaCHashObjectStore::<EliasFanoIndex>::build_index(a, bytes).unwrap()
    };
    assert_eq!(
        make(8).to_string(),
        "PaCHashObjectStore a=8 index=EliasFano"
    );

    let bytes =
        PaCHashObjectStore::<UncompressedBitVectorIndex>::write_to_file(vec![(1u64, vec![0u8; 8])])
            .unwrap();
    let ub = PaCHashObjectStore::<UncompressedBitVectorIndex>::build_index(1, bytes).unwrap();
    assert_eq!(
        ub.to_string(),
        "PaCHashObjectStore a=1 index=UncompressedBitVector"
    );

    let bytes =
        PaCHashObjectStore::<CompressedBitVectorIndex>::write_to_file(vec![(1u64, vec![0u8; 8])])
            .unwrap();
    let cb = PaCHashObjectStore::<CompressedBitVectorIndex>::build_index(16, bytes).unwrap();
    assert_eq!(
        cb.to_string(),
        "PaCHashObjectStore a=16 index=CompressedBitVector"
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

    assert_eq!(store.num_objects(), 20);
    assert!(store.num_blocks() >= 1);
    assert_eq!(store.data().len(), store.num_blocks() * BLOCK_LENGTH);
    // The bin multiplier surfaces through the Display label.
    assert!(store.to_string().contains("a=8"));
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
fn writer_counts_blocks_it_produces() {
    // Drive the writer directly and read the block count it reports. The count
    // must equal the block count the header records after finish.
    let mut writer = LinearObjectWriter::new();
    for key in 1..=200u64 {
        writer.write(key, &vec![0xABu8; 400]);
    }
    let generated_before_finish = writer.blocks_generated();
    assert!(generated_before_finish >= 1);

    let bytes = writer.finish(StoreMetadata::TYPE_PACHASH);
    let meta = StoreMetadata::from_bytes(&bytes).unwrap();
    // finish adds the terminator block, so the header count is at least what the
    // writer reported mid-stream.
    assert!(meta.num_blocks as usize >= generated_before_finish);
    assert_eq!(meta.num_blocks as usize, bytes.len() / BLOCK_LENGTH);
}
