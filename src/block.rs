//! Reading and writing the trailing table of a 4096-byte block.
//!
//! A block stores object data from byte 0 forward. The last bytes hold a table
//! that grows backward:
//!
//! ```text
//! [ object data .......... free ... | keys | offsets | emptyPageEnd | numObjects ]
//!   0                                                          4093       4094..4095
//! ```
//!
//! `keys` is `num_objects` little-endian `u64` values. `offsets` follows as
//! `num_objects` little-endian `u16` values. The last three bytes are the
//! 1-byte empty-page marker and the 2-byte object count.

use crate::config::{BLOCK_LENGTH, OVERHEAD_PER_BLOCK, OVERHEAD_PER_OBJECT};

/// A parsed view of one 4096-byte block.
///
/// Field meanings match the on-disk layout described in the module docs.
/// `offsets[i]` is the start byte of object `i` in the block for the PaCHash
/// table convention. See [`crate::writer`] and [`crate::reader`].
#[derive(Debug, Clone)]
pub struct BlockStorage {
    /// Number of objects whose table entry lives on this block.
    pub num_objects: u16,
    /// Unused tail bytes at the end of the data region for the last object.
    pub empty_page_end: u8,
    /// Byte offset where the table begins.
    pub table_start: usize,
    /// Start byte of each object in the block, in table order.
    pub offsets: Vec<u16>,
    /// Key of each object, in table order.
    pub keys: Vec<u64>,
}

impl BlockStorage {
    /// Parse a block from a 4096-byte slice.
    ///
    /// # Panics
    /// Panics when `data` is shorter than [`BLOCK_LENGTH`].
    pub fn parse(data: &[u8]) -> BlockStorage {
        assert!(data.len() >= BLOCK_LENGTH, "block must be 4096 bytes");
        let num_objects = u16::from_le_bytes([data[BLOCK_LENGTH - 2], data[BLOCK_LENGTH - 1]]);
        let empty_page_end = data[BLOCK_LENGTH - OVERHEAD_PER_BLOCK];
        let n = num_objects as usize;
        let table_start = BLOCK_LENGTH - OVERHEAD_PER_BLOCK - n * OVERHEAD_PER_OBJECT;

        let mut keys = Vec::with_capacity(n);
        for i in 0..n {
            let base = table_start + i * 8;
            keys.push(u64::from_le_bytes(data[base..base + 8].try_into().unwrap()));
        }
        let offsets_base = table_start + n * 8;
        let mut offsets = Vec::with_capacity(n);
        for i in 0..n {
            let base = offsets_base + i * 2;
            offsets.push(u16::from_le_bytes([data[base], data[base + 1]]));
        }

        BlockStorage {
            num_objects,
            empty_page_end,
            table_start,
            offsets,
            keys,
        }
    }

    /// Write the trailer of a block into `data`.
    ///
    /// Stores `num_objects`, `empty_page_end`, and the `keys` and `offsets`
    /// tables. The object data must already be in place. `data` must be at least
    /// [`BLOCK_LENGTH`] bytes.
    pub fn write_table(
        data: &mut [u8],
        num_objects: u16,
        empty_page_end: u8,
        keys: &[u64],
        offsets: &[u16],
    ) {
        assert!(data.len() >= BLOCK_LENGTH, "block must be 4096 bytes");
        let n = num_objects as usize;
        assert_eq!(keys.len(), n, "keys length must equal num_objects");
        assert_eq!(offsets.len(), n, "offsets length must equal num_objects");

        data[BLOCK_LENGTH - 2..BLOCK_LENGTH].copy_from_slice(&num_objects.to_le_bytes());
        data[BLOCK_LENGTH - OVERHEAD_PER_BLOCK] = empty_page_end;

        let table_start = BLOCK_LENGTH - OVERHEAD_PER_BLOCK - n * OVERHEAD_PER_OBJECT;
        for (i, key) in keys.iter().enumerate() {
            let base = table_start + i * 8;
            data[base..base + 8].copy_from_slice(&key.to_le_bytes());
        }
        let offsets_base = table_start + n * 8;
        for (i, off) in offsets.iter().enumerate() {
            let base = offsets_base + i * 2;
            data[base..base + 2].copy_from_slice(&off.to_le_bytes());
        }
    }

    /// Find a key in a block that uses the non-overlapping table convention.
    ///
    /// The Separator and Cuckoo writers store shifted cumulative offsets. Object
    /// 0 starts at 0. Object `i > 0` starts at `offsets[i - 1]`. The last entry
    /// holds the end of the last object. Returns `(length, start_offset)` on a
    /// hit or `None` when the key is absent.
    pub fn find_key_non_overlapping(&self, key: u64) -> Option<(usize, usize)> {
        for i in 0..self.num_objects as usize {
            if self.keys[i] == key {
                if i == 0 {
                    return Some((self.offsets[0] as usize, 0));
                } else {
                    let start = self.offsets[i - 1] as usize;
                    let len = self.offsets[i] as usize - start;
                    return Some((len, start));
                }
            }
        }
        None
    }
}
