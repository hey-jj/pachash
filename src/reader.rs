//! Sequential read-back of a store.
//!
//! [`LinearObjectReader`] walks every object in file order. It reconstructs
//! objects that span block boundaries by stitching their fragments and dropping
//! the table bytes in between. It is the round-trip oracle for the writer and
//! the source for [`crate::merge`].

use crate::block::BlockStorage;
use crate::config::{StoreMetadata, BLOCK_LENGTH};

/// One object returned by [`LinearObjectReader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// The object's 64-bit key.
    pub key: u64,
    /// The object's value bytes.
    pub value: Vec<u8>,
}

/// Reads objects from a store held in memory.
///
/// After construction the reader is positioned on the first real object. Read
/// [`current_key`](Self::current_key) and [`current_value`](Self::current_value),
/// then call [`advance`](Self::advance) to move on. Check
/// [`has_current`](Self::has_current) before reading.
#[derive(Debug)]
pub struct LinearObjectReader<'a> {
    data: &'a [u8],
    num_blocks: usize,
    current_block: usize,
    block: BlockStorage,
    current_element: usize,
    completed: bool,
    current_key: u64,
    current_value: Vec<u8>,
}

impl<'a> LinearObjectReader<'a> {
    /// Open a reader over a store's bytes.
    ///
    /// Reads the header, loads block 0, and skips the pseudo header object so
    /// the reader lands on the first real object. Returns an error when the
    /// metadata is invalid.
    pub fn new(data: &'a [u8]) -> Result<LinearObjectReader<'a>, crate::config::MetadataError> {
        let metadata = StoreMetadata::from_bytes(data)?;
        let num_blocks = metadata.num_blocks as usize;
        let block = BlockStorage::parse(&data[0..BLOCK_LENGTH]);
        let mut reader = LinearObjectReader {
            data,
            num_blocks,
            current_block: 0,
            block,
            current_element: 0,
            completed: false,
            current_key: 0,
            current_value: Vec::new(),
        };
        // Skip pseudo object 0 (the header). This lands on the first real
        // object, or on the terminator when the store is empty.
        reader.advance();
        Ok(reader)
    }

    /// Key of the object currently positioned.
    pub fn current_key(&self) -> u64 {
        self.current_key
    }

    /// Value of the object currently positioned.
    pub fn current_value(&self) -> &[u8] {
        &self.current_value
    }

    /// True while a real object is positioned and not yet consumed.
    pub fn has_current(&self) -> bool {
        !self.completed
    }

    fn load_block(&mut self, index: usize) {
        let start = index * BLOCK_LENGTH;
        self.block = BlockStorage::parse(&self.data[start..start + BLOCK_LENGTH]);
    }

    fn next_block(&mut self) {
        self.current_block += 1;
        self.load_block(self.current_block);
        self.current_element = usize::MAX;
    }

    /// Advance to the next object.
    ///
    /// Reconstructs across blocks when the current object is the block's last
    /// object and its data overlaps into following blocks. Marks the reader
    /// completed on the terminator or after the final object.
    pub fn advance(&mut self) {
        if self.completed || self.current_block == usize::MAX {
            self.completed = true;
            return;
        }
        self.current_element = self.current_element.wrapping_add(1);
        let elem = self.current_element;
        self.current_key = self.block.keys[elem];

        if elem < (self.block.num_objects as usize).wrapping_sub(1) {
            // Object stays inside this block.
            let start = self.block.offsets[elem] as usize;
            let end = self.block.offsets[elem + 1] as usize;
            let base = self.current_block * BLOCK_LENGTH;
            self.current_value = self.data[base + start..base + end].to_vec();
            return;
        }

        // Object is the last on the block. It may overlap into later blocks.
        let start = self.block.offsets[elem] as usize;
        let on_this_block = self.block.table_start - start - self.block.empty_page_end as usize;
        let base = self.current_block * BLOCK_LENGTH;
        let mut value = self.data[base + start..base + start + on_this_block].to_vec();

        if self.current_key == 0 {
            // The terminator object marks the end of real data.
            self.current_value = value;
            self.completed = true;
            return;
        }

        if self.current_block == self.num_blocks - 1 {
            // The last object exactly fills the last block. No terminator block.
            self.current_block = usize::MAX;
            self.current_value = value;
            return;
        }

        while self.current_block < self.num_blocks - 1 {
            self.next_block();
            let base = self.current_block * BLOCK_LENGTH;
            if self.block.num_objects > 0 {
                let len_on_next = self.block.offsets[0] as usize;
                value.extend_from_slice(&self.data[base..base + len_on_next]);
                self.current_value = value;
                return;
            } else {
                let keep = self.block.table_start - self.block.empty_page_end as usize;
                value.extend_from_slice(&self.data[base..base + keep]);
            }
        }
        self.current_value = value;
    }

    /// Collect every object in file order.
    pub fn read_all(mut self) -> Vec<Object> {
        let mut out = Vec::new();
        while self.has_current() {
            out.push(Object {
                key: self.current_key,
                value: self.current_value.clone(),
            });
            self.advance();
        }
        out
    }
}
