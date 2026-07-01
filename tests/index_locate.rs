//! The three predecessor indices must agree, and every key must be found.
//!
//! Agreement is checked over the states real construction produces. Each store
//! is built once, then opened with each of the three index types. For every
//! stored key the three stores must return the same value, and for known-absent
//! keys they must all miss.

mod common;

use pachash::{
    CompressedBitVectorIndex, EliasFanoIndex, Index, PaCHashObjectStore, UncompressedBitVectorIndex,
};

/// Build a store from `items`, open it with all three index types, and assert
/// every key resolves identically.
fn assert_three_indices_agree(a: u16, items: Vec<(u64, Vec<u8>)>) {
    let keys: Vec<u64> = items.iter().map(|(k, _)| *k).collect();
    let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items.clone()).unwrap();

    let ef = PaCHashObjectStore::<EliasFanoIndex>::build_index(a, bytes.clone()).unwrap();
    let ub =
        PaCHashObjectStore::<UncompressedBitVectorIndex>::build_index(a, bytes.clone()).unwrap();
    let cb = PaCHashObjectStore::<CompressedBitVectorIndex>::build_index(a, bytes).unwrap();

    for (key, value) in &items {
        let want = &value[..];
        assert_eq!(ef.query(*key).unwrap().value, want, "ef key={key}");
        assert_eq!(ub.query(*key).unwrap().value, want, "ub key={key}");
        assert_eq!(cb.query(*key).unwrap().value, want, "cb key={key}");
    }

    // A handful of keys not in the set must miss in all three.
    for probe in [u64::MAX, u64::MAX - 1, 0xDEAD_BEEF_0000_0001] {
        if keys.contains(&probe) {
            continue;
        }
        assert!(ef.query(probe).is_none(), "ef absent {probe}");
        assert!(ub.query(probe).is_none(), "ub absent {probe}");
        assert!(cb.query(probe).is_none(), "cb absent {probe}");
    }
}

#[test]
fn agree_small_values() {
    for a in [1u16, 2, 4, 8, 16] {
        assert_three_indices_agree(a, common::gen_items(60, 40));
    }
}

#[test]
fn agree_block_filling_values() {
    for a in [1u16, 4, 8] {
        assert_three_indices_agree(a, common::gen_items(40, 900));
    }
}

#[test]
fn agree_overlapping_values() {
    // Values large enough to span several blocks each.
    for a in [1u16, 8] {
        assert_three_indices_agree(a, common::gen_items(30, 12000));
    }
}

#[test]
fn agree_sparse_keys_empty_bins() {
    // Widely spaced keys create empty bins between blocks.
    let mut items = Vec::new();
    for i in 0..25u64 {
        let key = (i + 1).wrapping_mul(0x0100_0000_0000_0000).wrapping_add(1);
        items.push((key, vec![(i & 0xFF) as u8; 500]));
    }
    assert_three_indices_agree(8, items);
}

#[test]
fn direct_locate_single_block() {
    // A one-block file maps every bin to block 0.
    let build = |mut ix: Box<dyn Index>| {
        ix.push_back(0);
        ix.complete();
        for bin in 0..8 {
            assert_eq!(ix.locate(bin), (0, 1), "bin={bin}");
        }
    };
    build(Box::new(EliasFanoIndex::new(1, 8)));
    build(Box::new(UncompressedBitVectorIndex::new(1, 8)));
    build(Box::new(CompressedBitVectorIndex::new(1, 8)));
}

#[test]
fn direct_locate_strictly_increasing() {
    // Distinct bins are the common real case. All three must agree.
    let bins = [0usize, 1, 3, 6, 10];
    let num_blocks = bins.len();
    let num_bins = 16;

    let mut ef = EliasFanoIndex::new(num_blocks, num_bins);
    let mut ub = UncompressedBitVectorIndex::new(num_blocks, num_bins);
    let mut cb = CompressedBitVectorIndex::new(num_blocks, num_bins);
    for &b in &bins {
        ef.push_back(b);
        ub.push_back(b);
        cb.push_back(b);
    }
    ef.complete();
    ub.complete();
    cb.complete();

    for bin in 0..num_bins {
        let r = ef.locate(bin);
        assert_eq!(ub.locate(bin), r, "ub bin={bin}");
        assert_eq!(cb.locate(bin), r, "cb bin={bin}");
        assert!(r.0 + r.1 <= num_blocks);
    }
}
