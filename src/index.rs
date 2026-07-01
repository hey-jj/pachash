//! Predecessor indices mapping a bin to a block range.
//!
//! For each block the store pushes the first bin that intersects it. The values
//! are non-decreasing in block order. A query computes the bin of its key and
//! calls [`Index::locate`], which returns `(first_block, block_count)`: a
//! contiguous block range guaranteed to hold the key when it is present.
//!
//! Three variants implement the same mapping with different backings:
//! [`EliasFanoIndex`], [`UncompressedBitVectorIndex`], and
//! [`CompressedBitVectorIndex`]. They agree on every `locate` result.

/// A predecessor index over per-block bin values.
pub trait Index {
    /// Human-readable name of the backing structure.
    fn name() -> &'static str
    where
        Self: Sized;

    /// Append the bin value for the next block. Values must not decrease.
    fn push_back(&mut self, bin: usize);

    /// Finalize after all blocks are pushed.
    fn complete(&mut self);

    /// Return `(first_block, block_count)` covering `bin`.
    fn locate(&self, bin: usize) -> (usize, usize);

    /// Backing size in bytes.
    fn space(&self) -> usize;
}

/// A bit vector with linear rank and select.
///
/// The index is small, so linear scans are fine. `select0`/`select1` are
/// 1-based: `select1(k)` is the position of the `k`-th set bit, counting from 1.
struct BitVector {
    bits: Vec<bool>,
}

impl BitVector {
    fn new(len: usize) -> BitVector {
        BitVector {
            bits: vec![false; len],
        }
    }

    fn set(&mut self, index: usize) {
        self.bits[index] = true;
    }

    fn get(&self, index: usize) -> bool {
        self.bits.get(index).copied().unwrap_or(false)
    }

    /// Position of the `k`-th one bit, 1-based.
    fn select1(&self, k: usize) -> usize {
        let mut seen = 0;
        for (i, &b) in self.bits.iter().enumerate() {
            if b {
                seen += 1;
                if seen == k {
                    return i;
                }
            }
        }
        self.bits.len()
    }

    /// Position of the `k`-th zero bit, 1-based.
    fn select0(&self, k: usize) -> usize {
        let mut seen = 0;
        for (i, &b) in self.bits.iter().enumerate() {
            if !b {
                seen += 1;
                if seen == k {
                    return i;
                }
            }
        }
        self.bits.len()
    }

    /// Count of one bits in `bits[0..prefix]`.
    fn rank1(&self, prefix: usize) -> usize {
        self.bits[0..prefix.min(self.bits.len())]
            .iter()
            .filter(|&&b| b)
            .count()
    }

    fn space(&self) -> usize {
        self.bits.len().div_ceil(8)
    }
}

/// Elias-Fano style index. The default and most compact variant.
///
/// This stores the monotonic bin sequence directly. A width parameter would only
/// tune the encoding, not the query results, so it is omitted.
pub struct EliasFanoIndex {
    num_blocks: usize,
    values: Vec<usize>,
}

impl EliasFanoIndex {
    /// Create an index for a file with `num_blocks` blocks and `num_bins` bins.
    pub fn new(num_blocks: usize, _num_bins: usize) -> EliasFanoIndex {
        EliasFanoIndex {
            num_blocks,
            values: Vec::with_capacity(num_blocks),
        }
    }

    /// Position of the predecessor of `bin`.
    ///
    /// Returns the first index whose value equals the greatest value `<= bin`.
    /// Returning the first such index lets the exact-match back-up and the
    /// forward scan in [`locate`](Index::locate) span all equal values. When all
    /// values exceed `bin`, returns 0.
    fn predecessor_position(&self, bin: usize) -> usize {
        // Greatest value that is <= bin, if any.
        let pred_value = self.values.iter().copied().filter(|&v| v <= bin).max();
        match pred_value {
            Some(pv) => self.values.iter().position(|&v| v == pv).unwrap(),
            None => 0,
        }
    }
}

impl Index for EliasFanoIndex {
    fn name() -> &'static str {
        "EliasFano"
    }

    fn push_back(&mut self, bin: usize) {
        self.values.push(bin);
    }

    fn complete(&mut self) {}

    fn locate(&self, bin: usize) -> (usize, usize) {
        let mut i = self.predecessor_position(bin);
        let mut j = i;
        if self.values[i] == bin && i > 0 {
            i -= 1;
        }
        while j < self.num_blocks - 1 {
            let next = j + 1;
            if self.values[next] > bin {
                break;
            }
            j = next;
        }
        (i, j - i + 1)
    }

    fn space(&self) -> usize {
        self.values.len() * 8
    }
}

/// Uncompressed bit vector index.
///
/// Block `k` sets bit `k + bin_k`. This unary and gap encoding lets select
/// recover blocks from empty bins.
pub struct UncompressedBitVectorIndex {
    bit_vector: BitVector,
    num_pushed: usize,
    num_blocks: usize,
}

impl UncompressedBitVectorIndex {
    /// Create an index sized for `num_blocks` blocks and `num_bins` bins.
    pub fn new(num_blocks: usize, num_bins: usize) -> UncompressedBitVectorIndex {
        UncompressedBitVectorIndex {
            bit_vector: BitVector::new(num_blocks + num_bins),
            num_pushed: 0,
            num_blocks,
        }
    }
}

impl Index for UncompressedBitVectorIndex {
    fn name() -> &'static str {
        "UncompressedBitVector"
    }

    fn push_back(&mut self, bin: usize) {
        self.bit_vector.set(self.num_pushed + bin);
        self.num_pushed += 1;
    }

    fn complete(&mut self) {}

    fn locate(&self, bin: usize) -> (usize, usize) {
        let bv = &self.bit_vector;
        let possible_position_of_b = if bin == 0 { 0 } else { bv.select0(bin) + 1 };
        let array_index_of_predecessor = if bin == 0 {
            0
        } else {
            possible_position_of_b - bin - 1 + bv.get(possible_position_of_b) as usize
        };
        let mut bit_vector_index_of_predecessor = bv.select1(array_index_of_predecessor + 1);
        let value_of_predecessor = bit_vector_index_of_predecessor - array_index_of_predecessor;

        let mut i = array_index_of_predecessor;
        if value_of_predecessor == bin && i != 0 {
            i -= 1;
        }
        let mut j = array_index_of_predecessor;
        while bv.get(bit_vector_index_of_predecessor + 1) && j < self.num_blocks - 1 {
            j += 1;
            bit_vector_index_of_predecessor += 1;
        }
        (i, j - i + 1)
    }

    fn space(&self) -> usize {
        self.bit_vector.space()
    }
}

/// Compressed bit vector index.
///
/// Uses the same bit layout as [`UncompressedBitVectorIndex`] but a different
/// `locate` arithmetic based on rank and select. A block-compressed backing
/// would shrink the memory footprint without changing any query result, so this
/// keeps the plain backing and the compressed-path math.
pub struct CompressedBitVectorIndex {
    bit_vector: BitVector,
    num_pushed: usize,
}

impl CompressedBitVectorIndex {
    /// Create an index sized for `num_blocks` blocks and `num_bins` bins.
    pub fn new(num_blocks: usize, num_bins: usize) -> CompressedBitVectorIndex {
        CompressedBitVectorIndex {
            bit_vector: BitVector::new(num_blocks + num_bins),
            num_pushed: 0,
        }
    }
}

impl Index for CompressedBitVectorIndex {
    fn name() -> &'static str {
        "CompressedBitVector"
    }

    fn push_back(&mut self, bin: usize) {
        self.bit_vector.set(self.num_pushed + bin);
        self.num_pushed += 1;
    }

    fn complete(&mut self) {}

    fn locate(&self, bin: usize) -> (usize, usize) {
        let bv = &self.bit_vector;
        let possible_position_of_b = if bin == 0 { 0 } else { bv.select0(bin) + 1 };
        let array_index_of_predecessor = bv.rank1(possible_position_of_b + 1) - 1;
        let bit_vector_index_of_predecessor = bv.select1(array_index_of_predecessor + 1);
        let value_of_predecessor = bit_vector_index_of_predecessor - array_index_of_predecessor;

        let mut i = array_index_of_predecessor;
        if value_of_predecessor == bin && i != 0 {
            i -= 1;
        }
        let j = bv.select0(value_of_predecessor + 1) - (value_of_predecessor + 1);
        (i, j - i + 1)
    }

    fn space(&self) -> usize {
        self.bit_vector.space()
    }
}
