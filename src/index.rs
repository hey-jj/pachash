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
pub trait Index: std::fmt::Debug {
    /// Create an index sized for `num_blocks` blocks and `num_bins` bins.
    fn new(num_blocks: usize, num_bins: usize) -> Self
    where
        Self: Sized;

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
#[derive(Debug)]
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

/// Elias-Fano index. The default and most compact variant.
///
/// The monotonic bin sequence is split into a low part and a high part. Each
/// value's low `low_width` bits go into a packed array. The high bits form a
/// unary bit vector: value `k` sets bit `(value_k >> low_width) + k`. The two
/// parts together take about `2 + ceil(log2(num_bins / num_blocks))` bits per
/// block, near the bit-vector variants and far below one machine word.
#[derive(Debug)]
pub struct EliasFanoIndex {
    num_bins: usize,
    /// Low bits per value. Fixed after the first `push_back`.
    low_width: u32,
    /// Packed low bits, `low_width` per value, LSB-first across a `u64` stream.
    low: Vec<u64>,
    /// Unary high bits with a select structure.
    high: BitVector,
    /// Number of values pushed so far.
    len: usize,
    /// Values buffered before `complete` computes `low_width`.
    pending: Vec<usize>,
}

impl EliasFanoIndex {
    fn low_width_for(num_blocks: usize, num_bins: usize) -> u32 {
        if num_blocks == 0 {
            return 0;
        }
        // floor(log2(universe / n)), the standard Elias-Fano low width.
        let universe = num_bins.max(1) as u64;
        let n = num_blocks as u64;
        let ratio = universe / n;
        if ratio < 2 {
            0
        } else {
            63 - ratio.leading_zeros()
        }
    }

    fn push_low(&mut self, value: usize) {
        let width = self.low_width as usize;
        if width == 0 {
            return;
        }
        let low = (value as u64) & ((1u64 << width) - 1);
        let bit_pos = self.len * width;
        let word = bit_pos / 64;
        let shift = bit_pos % 64;
        while self.low.len() <= word + 1 {
            self.low.push(0);
        }
        self.low[word] |= low << shift;
        if shift + width > 64 {
            self.low[word + 1] |= low >> (64 - shift);
        }
    }

    fn low_at(&self, k: usize) -> u64 {
        let width = self.low_width as usize;
        if width == 0 {
            return 0;
        }
        let bit_pos = k * width;
        let word = bit_pos / 64;
        let shift = bit_pos % 64;
        let mask = (1u64 << width) - 1;
        let mut low = self.low[word] >> shift;
        if shift + width > 64 {
            low |= self.low[word + 1] << (64 - shift);
        }
        low & mask
    }

    /// Reconstruct the value stored at position `k`.
    fn value_at(&self, k: usize) -> usize {
        // The k-th one bit sits at (high_k + k); subtracting k gives high_k.
        let high = self.high.select1(k + 1) - k;
        (high << self.low_width) | self.low_at(k) as usize
    }

    /// Position of the rightmost block whose value is `<= bin`.
    ///
    /// Returns `None` when every value exceeds `bin`. The values are
    /// non-decreasing, so the last such position is the predecessor. Binary
    /// search over the decoded values keeps this at `O(log n)` selects.
    fn predecessor_position(&self, bin: usize) -> Option<usize> {
        if self.len == 0 || self.value_at(0) > bin {
            return None;
        }
        let mut lo = 0;
        let mut hi = self.len - 1;
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.value_at(mid) <= bin {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        Some(lo)
    }
}

impl Index for EliasFanoIndex {
    fn new(num_blocks: usize, num_bins: usize) -> EliasFanoIndex {
        EliasFanoIndex {
            num_bins,
            low_width: Self::low_width_for(num_blocks, num_bins),
            low: Vec::new(),
            high: BitVector::new(0),
            len: 0,
            pending: Vec::with_capacity(num_blocks),
        }
    }

    fn name() -> &'static str {
        "EliasFano"
    }

    fn push_back(&mut self, bin: usize) {
        self.pending.push(bin);
    }

    fn complete(&mut self) {
        let n = self.pending.len();
        let high_len = n + (self.num_bins >> self.low_width) + 1;
        self.high = BitVector::new(high_len);
        let values = std::mem::take(&mut self.pending);
        for (k, &value) in values.iter().enumerate() {
            self.len = k;
            self.push_low(value);
            let high = (value >> self.low_width) + k;
            self.high.set(high);
        }
        self.len = n;
    }

    fn locate(&self, bin: usize) -> (usize, usize) {
        // No block holds a value <= bin, so the target sits before the first
        // pushed bin. Resolve to block 0.
        let Some(j) = self.predecessor_position(bin) else {
            return (0, 1);
        };
        // j is the rightmost block whose value is <= bin, so it ends the range.
        let mut i = j;
        if self.value_at(j) == bin {
            // Exact hit. Back up over the run of equal values, then one more
            // block, since the matching object may spill from that neighbor.
            while i > 0 && self.value_at(i - 1) == bin {
                i -= 1;
            }
            i = i.saturating_sub(1);
        }
        (i, j - i + 1)
    }

    fn space(&self) -> usize {
        self.low.len() * 8 + self.high.space()
    }
}

/// Uncompressed bit vector index.
///
/// Block `k` sets bit `k + bin_k`. This unary and gap encoding lets select
/// recover blocks from empty bins.
#[derive(Debug)]
pub struct UncompressedBitVectorIndex {
    bit_vector: BitVector,
    num_pushed: usize,
    num_blocks: usize,
}

impl Index for UncompressedBitVectorIndex {
    fn new(num_blocks: usize, num_bins: usize) -> UncompressedBitVectorIndex {
        UncompressedBitVectorIndex {
            bit_vector: BitVector::new(num_blocks + num_bins),
            num_pushed: 0,
            num_blocks,
        }
    }

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
        // When bin sits below the first pushed value, no block is a predecessor
        // and the offset subtraction would underflow. Resolve to block 0.
        if bin > 0 && possible_position_of_b < bin + 1 {
            return (0, 1);
        }
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
#[derive(Debug)]
pub struct CompressedBitVectorIndex {
    bit_vector: BitVector,
    num_pushed: usize,
}

impl Index for CompressedBitVectorIndex {
    fn new(num_blocks: usize, num_bins: usize) -> CompressedBitVectorIndex {
        CompressedBitVectorIndex {
            bit_vector: BitVector::new(num_blocks + num_bins),
            num_pushed: 0,
        }
    }

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
        let rank = bv.rank1(possible_position_of_b + 1);
        // No set bit at or below this position means bin is below the first
        // pushed value. Resolve to block 0 instead of underflowing rank - 1.
        if rank == 0 {
            return (0, 1);
        }
        let array_index_of_predecessor = rank - 1;
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
