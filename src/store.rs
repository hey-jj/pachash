//! The PaCHash object store.
//!
//! [`PaCHashObjectStore`] builds a store from key and value pairs, then answers
//! point queries in one block-range read. Construction sorts items by their
//! 64-bit key, packs them fully into blocks, and builds a predecessor index that
//! maps a key's bin to the block range holding it.

use std::borrow::Cow;

use crate::block::BlockStorage;
use crate::config::{MetadataError, StoreMetadata, BLOCK_LENGTH};
use crate::hash::{key2bin, murmur_hash64};
use crate::index::Index;
use crate::writer::LinearObjectWriter;

/// Errors from building or querying a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// A stored key was 0. Key 0 is reserved for the file header.
    ReservedKey,
    /// The file header could not be read.
    Metadata(MetadataError),
    /// The file is not a PaCHash store.
    WrongType,
    /// The header declares more blocks than the file holds.
    TruncatedBody,
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::ReservedKey => write!(f, "key 0 is reserved for metadata"),
            StoreError::Metadata(e) => write!(f, "{e}"),
            StoreError::WrongType => write!(f, "opened file of wrong type"),
            StoreError::TruncatedBody => {
                write!(f, "file is shorter than its declared block count")
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<MetadataError> for StoreError {
    fn from(e: MetadataError) -> Self {
        StoreError::Metadata(e)
    }
}

/// A static store over variable-size objects, generic over its index.
pub struct PaCHashObjectStore<I: Index> {
    /// The bin multiplier. `num_bins = num_blocks * a`.
    a: u16,
    data: Vec<u8>,
    index: I,
    num_blocks: usize,
    num_bins: usize,
    max_size: usize,
    num_objects: usize,
}

impl<I: Index> std::fmt::Debug for PaCHashObjectStore<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaCHashObjectStore")
            .field("a", &self.a)
            .field("num_blocks", &self.num_blocks)
            .field("num_bins", &self.num_bins)
            .field("max_size", &self.max_size)
            .field("num_objects", &self.num_objects)
            .finish_non_exhaustive()
    }
}

/// Human-readable label naming the bin multiplier and index kind.
impl<I: Index> std::fmt::Display for PaCHashObjectStore<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PaCHashObjectStore a={} index={}", self.a, I::name())
    }
}

impl<I: Index> PaCHashObjectStore<I> {
    /// Bin of a key for this store's bin count.
    pub fn key2bin(&self, key: u64) -> usize {
        key2bin(key, self.num_bins as u64) as usize
    }

    /// Number of blocks in the file.
    pub fn num_blocks(&self) -> usize {
        self.num_blocks
    }

    /// Number of objects in the file, excluding the header and terminator.
    pub fn num_objects(&self) -> usize {
        self.num_objects
    }

    /// Largest object length in the file.
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Index space in bytes.
    pub fn index_space(&self) -> usize {
        self.index.space()
    }

    /// Build the store bytes from key and value pairs.
    ///
    /// Items are sorted by 64-bit key, then packed. A key of 0 is rejected. The
    /// caller is responsible for using distinct keys. The bin multiplier `a`
    /// only affects the index built later, so it is not needed here.
    pub fn write_to_file(mut items: Vec<(u64, Vec<u8>)>) -> Result<Vec<u8>, StoreError> {
        for (key, _) in &items {
            if *key == 0 {
                return Err(StoreError::ReservedKey);
            }
        }
        items.sort_by_key(|(key, _)| *key);

        let mut writer = LinearObjectWriter::new();
        for (key, value) in &items {
            writer.write(*key, value);
        }
        Ok(writer.finish(StoreMetadata::TYPE_PACHASH))
    }

    /// Build the store from string pairs using the default key hash.
    ///
    /// Keys are hashed with [`murmur_hash64`], the same as the string query
    /// path.
    pub fn write_to_file_strings(items: &[(String, String)]) -> Result<Vec<u8>, StoreError> {
        let hashed: Vec<(u64, Vec<u8>)> = items
            .iter()
            .map(|(k, v)| (murmur_hash64(k.as_bytes()), v.as_bytes().to_vec()))
            .collect();
        Self::write_to_file(hashed)
    }

    /// Open a store's bytes and build the in-memory index.
    ///
    /// Returns an error when the header is bad or the type is not PaCHash.
    pub fn build_index(a: u16, data: Vec<u8>) -> Result<PaCHashObjectStore<I>, StoreError> {
        let metadata = StoreMetadata::from_bytes(&data)?;
        if metadata.kind != StoreMetadata::TYPE_PACHASH {
            return Err(StoreError::WrongType);
        }
        let num_blocks = metadata.num_blocks as usize;
        if num_blocks > data.len() / BLOCK_LENGTH {
            return Err(StoreError::TruncatedBody);
        }
        let max_size = metadata.max_size as usize;
        let num_bins = num_blocks * a as usize;

        let mut index = I::new(num_blocks, num_bins);
        let mut real_objects = 0;
        let mut last_key_in_previous_block: u64 = 0;

        for blocks_read in 0..num_blocks {
            let start = blocks_read * BLOCK_LENGTH;
            let block = BlockStorage::parse(&data[start..start + BLOCK_LENGTH]);

            let last_bin_in_previous_block =
                key2bin(last_key_in_previous_block, num_bins as u64) as usize;
            if block.num_objects > 0 && block.offsets[0] == 0 {
                let first_bin_in_this_block = key2bin(block.keys[0], num_bins as u64) as usize;
                if first_bin_in_this_block > last_bin_in_previous_block {
                    index.push_back(first_bin_in_this_block - 1);
                } else {
                    index.push_back(last_bin_in_previous_block);
                }
            } else {
                index.push_back(last_bin_in_previous_block);
            }

            if block.num_objects > 0 {
                let key = block.keys[block.num_objects as usize - 1];
                debug_assert!(key > last_key_in_previous_block || blocks_read == num_blocks - 1);
                last_key_in_previous_block = key;
            }
            // Count real objects. Key 0 marks the header and the terminator.
            real_objects += block.keys.iter().filter(|&&k| k != 0).count();
        }

        index.complete();
        let num_objects = real_objects;

        Ok(PaCHashObjectStore {
            a,
            data,
            index,
            num_blocks,
            num_bins,
            max_size,
            num_objects,
        })
    }

    /// Buffer size a query needs to hold a worst-case multi-block object.
    pub fn required_buffer_per_query(&self) -> usize {
        4 * (self.max_size + BLOCK_LENGTH - 1)
    }

    /// IO operations a query needs.
    pub fn required_ios_per_query(&self) -> usize {
        1
    }

    /// Look up a key.
    ///
    /// Returns the value bytes when present, or `None` when absent. A value that
    /// fits in one block borrows from the store. A value stitched across blocks
    /// owns a fresh buffer, so the return type is a [`Cow`].
    pub fn query(&self, key: u64) -> Option<Cow<'_, [u8]>> {
        let bin = self.key2bin(key);
        let (first_block, blocks_accessed) = self.index.locate(bin);

        // The loaded window is `blocks_accessed` blocks starting at first_block.
        let window_start = first_block * BLOCK_LENGTH;
        let window = &self.data[window_start..window_start + blocks_accessed * BLOCK_LENGTH];

        for block_idx in 0..blocks_accessed {
            let block_ptr = block_idx * BLOCK_LENGTH;
            let block = BlockStorage::parse(&window[block_ptr..block_ptr + BLOCK_LENGTH]);
            for i in 0..block.num_objects as usize {
                if block.keys[i] == key {
                    return Some(self.reconstruct(window, i, &block, block_idx, blocks_accessed));
                }
            }
        }
        None
    }

    /// Rebuild the value for a matched key, stitching across blocks if needed.
    fn reconstruct<'w>(
        &self,
        window: &'w [u8],
        i: usize,
        block: &BlockStorage,
        mut block_idx: usize,
        blocks_accessed: usize,
    ) -> Cow<'w, [u8]> {
        let block_ptr = block_idx * BLOCK_LENGTH;
        if i < block.num_objects as usize - 1 {
            let start = block.offsets[i] as usize;
            let end = block.offsets[i + 1] as usize;
            return Cow::Borrowed(&window[block_ptr + start..block_ptr + end]);
        }

        // The object is the block's last object. It overlaps into later blocks.
        let start = block.offsets[i] as usize;
        let on_this_block = block.table_start - start - block.empty_page_end as usize;
        let last_ptr = block_ptr + start;
        // A single-block last object still fits its own slice.
        if block_idx + 1 >= blocks_accessed {
            return Cow::Borrowed(&window[last_ptr..last_ptr + on_this_block]);
        }

        let mut value = window[last_ptr..last_ptr + on_this_block].to_vec();
        block_idx += 1;
        while block_idx < blocks_accessed {
            let next_ptr = block_idx * BLOCK_LENGTH;
            let next = BlockStorage::parse(&window[next_ptr..next_ptr + BLOCK_LENGTH]);
            if next.num_objects > 0 {
                let len_on_next = next.offsets[0] as usize;
                value.extend_from_slice(&window[next_ptr..next_ptr + len_on_next]);
                return Cow::Owned(value);
            } else {
                let len_on_next = next.table_start;
                let keep = len_on_next - next.empty_page_end as usize;
                value.extend_from_slice(&window[next_ptr..next_ptr + keep]);
            }
            block_idx += 1;
        }
        Cow::Owned(value)
    }

    /// Look up a string key using the default hash.
    pub fn query_string(&self, key: &str) -> Option<Cow<'_, [u8]>> {
        self.query(murmur_hash64(key.as_bytes()))
    }

    /// Access to the store's raw bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
