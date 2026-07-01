//! Shared helpers for the conformance suite.
//!
//! The generator is a fixed splitmix64 so inputs are stable and independent of
//! any platform RNG. Object sizes span the regimes the store must handle: small
//! values that fit one block, values sized to fill a block, and values that
//! overlap several blocks.

#![allow(dead_code)]

/// splitmix64 seed used across all generated inputs.
pub const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// One splitmix64 step. Mutates `state` and returns the next value.
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Generate `n` items with distinct integer keys and byte values.
///
/// Key `i` is `i + 1` so no key is 0. Value `i` is `length_i` copies of the
/// byte `i & 0xFF`, with `length_i` drawn from splitmix64 and bounded by
/// `max_len`.
pub fn gen_items(n: usize, max_len: usize) -> Vec<(u64, Vec<u8>)> {
    let mut state = SEED;
    let mut items = Vec::with_capacity(n);
    for i in 0..n {
        let len = (splitmix64(&mut state) as usize) % (max_len + 1);
        let byte = (i & 0xFF) as u8;
        items.push(((i + 1) as u64, vec![byte; len]));
    }
    items
}

/// Report the first byte offset where two slices differ, for diff messages.
pub fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    if a.len() != b.len() {
        return Some(a.len().min(b.len()));
    }
    a.iter().zip(b.iter()).position(|(x, y)| x != y)
}
