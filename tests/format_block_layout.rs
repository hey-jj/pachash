//! On-disk format parity: the 4096-byte block trailer and the metadata header.

mod common;

use pachash::{
    BlockStorage, EliasFanoIndex, PaCHashObjectStore, StoreMetadata, BLOCK_LENGTH,
    OVERHEAD_PER_BLOCK, OVERHEAD_PER_OBJECT, STORE_METADATA_SIZE,
};

#[test]
fn constants_match_layout() {
    assert_eq!(BLOCK_LENGTH, 4096);
    assert_eq!(OVERHEAD_PER_OBJECT, 10);
    assert_eq!(OVERHEAD_PER_BLOCK, 3);
    assert_eq!(STORE_METADATA_SIZE, 56);
}

#[test]
fn metadata_byte_offsets() {
    // The header image must place fields at the LP64 struct offsets: magic at 0,
    // version at 32, kind at 34, num_blocks at 40, max_size at 48.
    let meta = StoreMetadata {
        kind: StoreMetadata::TYPE_PACHASH,
        num_blocks: 0x1122_3344_5566_7788,
        max_size: 0x99AA_BBCC_DDEE_FF00,
    };
    let bytes = meta.to_bytes();
    assert_eq!(&bytes[0..32], b"Variable size object store file\0");
    assert_eq!(bytes[32], 1); // version
    assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 1000);
    assert_eq!(
        u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
        0x1122_3344_5566_7788
    );
    assert_eq!(
        u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
        0x99AA_BBCC_DDEE_FF00
    );
    // Padding bytes at 36..40 stay zero.
    assert_eq!(&bytes[36..40], &[0, 0, 0, 0]);
}

#[test]
fn metadata_round_trips() {
    let meta = StoreMetadata {
        kind: StoreMetadata::TYPE_SEPARATOR + 6,
        num_blocks: 42,
        max_size: 5000,
    };
    let parsed = StoreMetadata::from_bytes(&meta.to_bytes()).unwrap();
    assert_eq!(parsed, meta);
}

#[test]
fn block_trailer_positions() {
    let items = common::gen_items(20, 300);
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();

    // Block 0 holds the header as its first object.
    let block0 = BlockStorage::parse(&bytes[0..BLOCK_LENGTH]);
    assert!(block0.num_objects >= 1);
    assert_eq!(block0.keys[0], 0, "first object of block 0 is metadata");
    assert_eq!(block0.offsets[0], 0, "header starts at byte 0");

    // The count lives in the last two bytes, the empty marker one byte before.
    let n = block0.num_objects;
    assert_eq!(
        u16::from_le_bytes([bytes[BLOCK_LENGTH - 2], bytes[BLOCK_LENGTH - 1]]),
        n
    );

    // The table sits at the computed offset.
    let expected_table_start = BLOCK_LENGTH - OVERHEAD_PER_BLOCK - n as usize * OVERHEAD_PER_OBJECT;
    assert_eq!(block0.table_start, expected_table_start);
}

#[test]
fn header_carries_block_count() {
    let items = common::gen_items(50, 200);
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let meta = StoreMetadata::from_bytes(&bytes).unwrap();
    assert_eq!(meta.kind, StoreMetadata::TYPE_PACHASH);
    assert_eq!(meta.num_blocks as usize, bytes.len() / BLOCK_LENGTH);
}

#[test]
fn file_length_is_block_multiple() {
    for n in [0usize, 1, 5, 50] {
        let items = common::gen_items(n, 150);
        let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
        assert_eq!(bytes.len() % BLOCK_LENGTH, 0, "n={n}");
        assert!(!bytes.is_empty(), "n={n}");
    }
}
