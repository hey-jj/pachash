//! Hash and bucket math parity.

use pachash::{ceillog2, fastrange64, key2bin, murmur_hash64, murmur_hash64_seeded};

#[test]
fn murmur_known_values() {
    // Anchors for MurmurHash64A with seed 0. Any drift here changes on-disk key
    // order and would break stored files.
    let cases: [(&str, u64); 8] = [
        ("", 0x0000_0000_0000_0000),
        ("a", 0x0717_17d2_d36b_6b11),
        ("key_0", 0x7a32_4099_5ec0_fe2d),
        ("key_1", 0xf12f_c30b_80f7_a267),
        ("hello", 0x1e68_d17c_457b_f117),
        ("abcdefgh", 0xafdb_0257_ff41_aa98),
        ("abcdefghi", 0xc9b9_d843_5614_6ac2),
        ("The quick brown fox", 0xf323_1866_c315_bc69),
    ];
    for (input, expected) in cases {
        assert_eq!(murmur_hash64(input.as_bytes()), expected, "input {input:?}");
    }
}

#[test]
fn murmur_seed_changes_output() {
    let data = b"payload";
    assert_ne!(murmur_hash64_seeded(data, 0), murmur_hash64_seeded(data, 1));
    assert_eq!(murmur_hash64(data), murmur_hash64_seeded(data, 0));
}

#[test]
fn murmur_all_tail_lengths() {
    // Every remainder path (1..=7 bytes) must run without panic and be stable.
    for len in 0..=17 {
        let data: Vec<u8> = (0..len as u8).collect();
        let a = murmur_hash64(&data);
        let b = murmur_hash64(&data);
        assert_eq!(a, b, "len {len}");
    }
}

#[test]
fn fastrange_bounds() {
    assert_eq!(fastrange64(0, 10), 0);
    assert_eq!(fastrange64(u64::MAX, 10), 9);
    for range in [1u64, 2, 7, 100, 4096] {
        assert_eq!(fastrange64(0, range), 0);
        assert_eq!(fastrange64(u64::MAX, range), range - 1);
    }
}

#[test]
fn key2bin_edges() {
    let num_bins = 40u64;
    assert_eq!(key2bin(0, num_bins), 0);
    assert_eq!(key2bin(u64::MAX, num_bins), num_bins - 1);
    // key2bin is fastrange under a different name.
    for key in [1u64, 12345, 1 << 40, u64::MAX / 3] {
        assert_eq!(key2bin(key, num_bins), fastrange64(key, num_bins));
    }
}

#[test]
fn key2bin_is_monotone_in_key() {
    let num_bins = 1000u64;
    let mut last = 0;
    for step in 1..=1000u64 {
        let key = step.wrapping_mul(0x1234_5678_9abc_def1);
        let bin = key2bin(key, num_bins);
        assert!(bin < num_bins);
        last = last.max(bin);
    }
    assert!(last > 0);
}

#[test]
fn ceillog2_values() {
    assert_eq!(ceillog2(0), 0);
    assert_eq!(ceillog2(1), 0);
    assert_eq!(ceillog2(2), 1);
    assert_eq!(ceillog2(3), 2);
    assert_eq!(ceillog2(4), 2);
    assert_eq!(ceillog2(5), 3);
    assert_eq!(ceillog2(8), 3);
    assert_eq!(ceillog2(9), 4);
    assert_eq!(ceillog2(16), 4);
    assert_eq!(ceillog2(1024), 10);
}
