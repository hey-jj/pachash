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
        assert_eq!(ef.query(*key).unwrap().as_ref(), want, "ef key={key}");
        assert_eq!(ub.query(*key).unwrap().as_ref(), want, "ub key={key}");
        assert_eq!(cb.query(*key).unwrap().as_ref(), want, "cb key={key}");
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

/// A `(first_block, count)` range returned by `locate`.
type Range = (usize, usize);

/// Locate a bin with each index and return all three results.
fn locate_all(values: &[usize], num_bins: usize, bin: usize) -> [Range; 3] {
    let nb = values.len();
    let mut ef = EliasFanoIndex::new(nb, num_bins);
    let mut ub = UncompressedBitVectorIndex::new(nb, num_bins);
    let mut cb = CompressedBitVectorIndex::new(nb, num_bins);
    for &v in values {
        ef.push_back(v);
        ub.push_back(v);
        cb.push_back(v);
    }
    ef.complete();
    ub.complete();
    cb.complete();
    [ef.locate(bin), ub.locate(bin), cb.locate(bin)]
}

#[test]
fn locate_canonical_values() {
    // Expected (first_block, count) for known bin sequences. The values come from
    // the predecessor arithmetic: the predecessor of a bin is the rightmost block
    // whose value is at most the bin. On an exact match the range backs up one
    // block to the left, then extends right over blocks that share the value.
    //
    // Table rows: (values, num_bins, bin, expected).
    type Row = (&'static [usize], usize, usize, Range);
    let cases: &[Row] = &[
        // A single block covers every bin.
        (&[0], 4, 0, (0, 1)),
        (&[0], 4, 3, (0, 1)),
        // Strictly increasing bins, distinct blocks.
        (&[0, 1, 3], 6, 0, (0, 1)),
        (&[0, 1, 3], 6, 1, (0, 2)),
        (&[0, 1, 3], 6, 2, (1, 1)),
        (&[0, 1, 3], 6, 3, (1, 2)),
        (&[0, 1, 3], 6, 5, (2, 1)),
    ];
    for &(values, num_bins, bin, expected) in cases {
        let [ef, ub, cb] = locate_all(values, num_bins, bin);
        assert_eq!(ef, expected, "EF values={values:?} bin={bin}");
        assert_eq!(ub, expected, "UB values={values:?} bin={bin}");
        assert_eq!(cb, expected, "CB values={values:?} bin={bin}");
    }
}

#[test]
fn locate_duplicate_bins_present_key_regime() {
    // When every block shares the same bin value, a query for that exact bin must
    // return the whole run so the scan can find any key inside it. This is the
    // regime that real construction produces when all keys are tiny.
    let values = [0usize, 0, 0, 0, 0];
    let num_bins = 5;
    // Bin 0 is the shared value. All three return the full run of blocks.
    let [ef, ub, cb] = locate_all(&values, num_bins, 0);
    assert_eq!(ef, (0, 5), "EF bin=0");
    assert_eq!(ub, (0, 5), "UB bin=0");
    assert_eq!(cb, (0, 5), "CB bin=0");
}

#[test]
fn locate_gap_bins_over_repeated_values_agree() {
    // A query bin above a run of equal values must land on the rightmost block
    // of the run, not the whole run. This is the case where the Elias-Fano
    // predecessor once picked the leftmost match and read the whole file.
    let values = [0usize, 0, 0, 0, 0];
    let num_bins = 5;
    for bin in 1..num_bins {
        let [ef, ub, cb] = locate_all(&values, num_bins, bin);
        assert_eq!(ef, (4, 1), "EF bin={bin}");
        assert_eq!(ub, (4, 1), "UB bin={bin}");
        assert_eq!(cb, (4, 1), "CB bin={bin}");
    }

    // A mixed run: bins that fall in the gap after the [2,2] run resolve to the
    // rightmost block whose value is at most the query bin.
    let values = [0usize, 2, 2, 5];
    let num_bins = 12;
    let expected = [
        (0, (0, 1)),
        (1, (0, 1)),
        (2, (0, 3)),
        (3, (2, 1)),
        (4, (2, 1)),
        (5, (2, 2)),
        (6, (3, 1)),
        (11, (3, 1)),
    ];
    for (bin, want) in expected {
        let [ef, ub, cb] = locate_all(&values, num_bins, bin);
        assert_eq!(ef, want, "EF values={values:?} bin={bin}");
        assert_eq!(ub, want, "UB values={values:?} bin={bin}");
        assert_eq!(cb, want, "CB values={values:?} bin={bin}");
    }
}

#[test]
fn locate_first_bin_above_zero_never_panics() {
    // Building an index whose first block bin is above 0 and querying a bin below
    // it must resolve to block 0, not underflow. Stores never produce this state,
    // but the index types are public and a caller can.
    for &first in &[1usize, 2, 5] {
        for &nb in &[1usize, 3] {
            let values: Vec<usize> = (0..nb).map(|k| first + k).collect();
            let num_bins = first + nb + 4;
            for bin in 0..num_bins {
                let [ef, ub, cb] = locate_all(&values, num_bins, bin);
                for (name, (i, count)) in [("ef", ef), ("ub", ub), ("cb", cb)] {
                    assert!(count >= 1, "{name} empty range values={values:?} bin={bin}");
                    assert!(
                        i + count <= nb,
                        "{name} out of range values={values:?} bin={bin} -> ({i},{count})"
                    );
                }
                // Below the first value every index resolves to block 0.
                if bin < first {
                    assert_eq!(ef, (0, 1), "ef bin={bin}");
                    assert_eq!(ub, (0, 1), "ub bin={bin}");
                    assert_eq!(cb, (0, 1), "cb bin={bin}");
                }
            }
        }
    }
}

#[test]
fn elias_fano_backing_is_compact() {
    // The default index must not cost a machine word per block. Its backing
    // stays within a small factor of the bit-vector variants for a monotonic
    // sequence.
    let num_blocks = 1000;
    let num_bins = 1000;
    let mut ef = EliasFanoIndex::new(num_blocks, num_bins);
    let mut ub = UncompressedBitVectorIndex::new(num_blocks, num_bins);
    for k in 0..num_blocks {
        ef.push_back(k);
        ub.push_back(k);
    }
    ef.complete();
    ub.complete();
    // 64 bits per block would be 8000 bytes. The Elias-Fano split is far under.
    assert!(
        ef.space() <= 3 * ub.space(),
        "ef={} ub={}",
        ef.space(),
        ub.space()
    );
    assert!(
        ef.space() * 8 < num_blocks * 8,
        "ef under one word per block"
    );
}
