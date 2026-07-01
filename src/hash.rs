//! Hashing and bucket math.
//!
//! [`murmur_hash64`] maps a byte slice to a 64-bit key. [`fastrange64`] maps a
//! 64-bit value uniformly into a range without modulo bias. [`key2bin`] uses it
//! to place a key into one of `num_bins` bins.

const M: u64 = 0xc6a4_a793_5bd1_e995;
const R: u32 = 47;

/// Default seed for [`murmur_hash64`], matching the seeded variant with seed 0.
pub const DEFAULT_SEED: u64 = 0;

/// 64-bit MurmurHash2 (the `MurmurHash64A` variant) over `data` with a seed.
///
/// The result drives on-disk key order, so the byte and tail handling must stay
/// fixed.
pub fn murmur_hash64_seeded(data: &[u8], seed: u64) -> u64 {
    let len = data.len();
    let mut h: u64 = seed ^ (len as u64).wrapping_mul(M);

    let n_blocks = len / 8;
    for i in 0..n_blocks {
        let mut k = u64::from_le_bytes(data[i * 8..i * 8 + 8].try_into().unwrap());
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        h ^= k;
        h = h.wrapping_mul(M);
    }

    let tail = &data[n_blocks * 8..];
    let rem = len & 7;
    if rem >= 7 {
        h ^= (tail[6] as u64) << 48;
    }
    if rem >= 6 {
        h ^= (tail[5] as u64) << 40;
    }
    if rem >= 5 {
        h ^= (tail[4] as u64) << 32;
    }
    if rem >= 4 {
        h ^= (tail[3] as u64) << 24;
    }
    if rem >= 3 {
        h ^= (tail[2] as u64) << 16;
    }
    if rem >= 2 {
        h ^= (tail[1] as u64) << 8;
    }
    if rem >= 1 {
        h ^= tail[0] as u64;
        h = h.wrapping_mul(M);
    }

    h ^= h >> R;
    h = h.wrapping_mul(M);
    h ^= h >> R;
    h
}

/// [`murmur_hash64_seeded`] with the default seed.
pub fn murmur_hash64(data: &[u8]) -> u64 {
    murmur_hash64_seeded(data, DEFAULT_SEED)
}

/// Map `value` uniformly into `[0, range)` using a 128-bit multiply.
///
/// This is Lemire's fastrange. It avoids the bias of a plain modulo.
pub fn fastrange64(value: u64, range: u64) -> u64 {
    (((value as u128) * (range as u128)) >> 64) as u64
}

/// Bin of a key given the total bin count.
///
/// Equivalent to [`fastrange64`]. Kept separate to mirror the store's naming.
pub fn key2bin(key: u64, num_bins: u64) -> u64 {
    fastrange64(key, num_bins)
}

/// Number of bits needed to represent `x`, that is `ceil(log2(x))`.
///
/// Returns 0 for inputs 0 and 1. Used to size the Elias-Fano lower part.
pub fn ceillog2(x: u64) -> u32 {
    if x <= 1 {
        0
    } else {
        64 - (x - 1).leading_zeros()
    }
}
