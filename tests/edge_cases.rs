//! Edge cases from the spec: reserved keys, empty input, terminator branches,
//! empty bins, and header validation.

mod common;

use pachash::{
    EliasFanoIndex, LinearObjectReader, MetadataError, PaCHashObjectStore, StoreError,
    StoreMetadata, BLOCK_LENGTH,
};

#[test]
fn reserved_key_zero_is_rejected() {
    // E1: key 0 is reserved for the header.
    let items = vec![(0u64, b"illegal".to_vec()), (1u64, b"fine".to_vec())];
    let err = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap_err();
    assert_eq!(err, StoreError::ReservedKey);
}

#[test]
fn empty_input_builds_valid_store() {
    // E2: no objects. One header block, zero objects, all queries miss.
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(vec![]).unwrap();
    assert_eq!(bytes.len() % BLOCK_LENGTH, 0);
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes.clone()).unwrap();
    assert_eq!(store.num_objects(), 0);

    let reader = LinearObjectReader::new(&bytes).unwrap();
    assert_eq!(reader.read_all().len(), 0);
}

#[test]
fn single_small_object_terminator_path() {
    // E3: one small object. Round-trips through the terminator branch.
    let items = vec![(5u64, b"tiny".to_vec())];
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();
    assert_eq!(store.query(5).unwrap().as_ref(), b"tiny");
    assert_eq!(store.num_objects(), 1);
}

#[test]
fn close_terminator_both_branches() {
    // E6: force each close branch and check both round-trip.
    // Branch A: a block left with more than 128 free bytes writes a terminator.
    let items_a = vec![(1u64, vec![1u8; 10]), (2u64, vec![2u8; 10])];
    let store_a = build(items_a.clone());
    for (k, v) in &items_a {
        assert_eq!(store_a.query(*k).unwrap().as_ref(), &v[..]);
    }

    // Branch B: fill a block so at most 128 bytes remain, then close flushes
    // as-is without a terminator object. Sweep sizes to land in the window.
    for target in (BLOCK_LENGTH - 260)..(BLOCK_LENGTH - 60) {
        let value = vec![0x7u8; target];
        let store_b = build(vec![(3u64, value.clone())]);
        assert_eq!(
            store_b.query(3).unwrap().as_ref(),
            &value[..],
            "target={target}"
        );
    }
}

#[test]
fn empty_bins_between_blocks() {
    // E10: widely spaced keys leave empty bins between blocks. The index still
    // resolves every key.
    let mut items = Vec::new();
    for i in 0..30u64 {
        let key = (i + 1).wrapping_mul(0x0010_0000_0000_0000).wrapping_add(7);
        items.push((key, vec![(i & 0xFF) as u8; 800]));
    }
    let store = build(items.clone());
    for (key, value) in &items {
        assert_eq!(store.query(*key).unwrap().as_ref(), &value[..], "key {key}");
    }
}

#[test]
fn wrong_type_file_rejected() {
    // E11: a store with a non-PaCHash type is refused by build_index.
    let bytes =
        PaCHashObjectStore::<EliasFanoIndex>::write_to_file(common::gen_items(5, 50)).unwrap();
    let mut tampered = bytes.clone();
    // Overwrite the type field with the Cuckoo type.
    let cuckoo = StoreMetadata::TYPE_CUCKOO.to_le_bytes();
    tampered[34] = cuckoo[0];
    tampered[35] = cuckoo[1];
    match PaCHashObjectStore::<EliasFanoIndex>::build_index(8, tampered) {
        Err(e) => assert_eq!(e, StoreError::WrongType),
        Ok(_) => panic!("wrong-type file should be rejected"),
    }
}

#[test]
fn bad_magic_rejected() {
    let mut bytes =
        PaCHashObjectStore::<EliasFanoIndex>::write_to_file(common::gen_items(3, 40)).unwrap();
    bytes[0] = b'X';
    match PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes) {
        Err(e) => assert_eq!(e, StoreError::Metadata(MetadataError::BadMagic)),
        Ok(_) => panic!("bad magic should be rejected"),
    }
}

#[test]
fn bad_version_rejected() {
    let mut bytes =
        PaCHashObjectStore::<EliasFanoIndex>::write_to_file(common::gen_items(3, 40)).unwrap();
    bytes[32] = 99;
    let err = StoreMetadata::from_bytes(&bytes).unwrap_err();
    assert_eq!(err, MetadataError::BadVersion(99));
}

#[test]
fn truncated_file_rejected() {
    let err = StoreMetadata::from_bytes(&[0u8; 10]).unwrap_err();
    assert_eq!(err, MetadataError::Truncated);
}

fn build(items: Vec<(u64, Vec<u8>)>) -> PaCHashObjectStore<EliasFanoIndex> {
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
    PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap()
}
