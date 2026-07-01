//! The PaCHash packer.
//!
//! [`LinearObjectWriter`] appends objects into 4096-byte blocks packed fully.
//! An object that does not fit continues onto the next block. Each block gets a
//! table of keys and start offsets. The first object of block 0 holds the file
//! header, patched with the final block count on [`LinearObjectWriter::finish`].

use crate::block::BlockStorage;
use crate::config::{
    StoreMetadata, BLOCK_LENGTH, OVERHEAD_PER_BLOCK, OVERHEAD_PER_OBJECT, STORE_METADATA_SIZE,
};

/// Builds a store in memory, one object at a time.
///
/// Call [`write`](LinearObjectWriter::write) for each object in key order, then
/// [`finish`](LinearObjectWriter::finish) to get the file bytes.
pub struct LinearObjectWriter {
    /// Finished blocks, each exactly [`BLOCK_LENGTH`] bytes.
    blocks: Vec<u8>,
    /// The block currently being filled.
    current: Vec<u8>,
    keys: Vec<u64>,
    offsets: Vec<u16>,
    num_objects_on_page: usize,
    space_left_on_block: usize,
    block_writing_position: usize,
    max_size: usize,
    /// Number of full blocks produced so far.
    pub blocks_generated: usize,
}

impl Default for LinearObjectWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl LinearObjectWriter {
    /// Create a writer and reserve block 0 for the header.
    ///
    /// A dummy header object with key 0 is written first. Its bytes are patched
    /// on [`finish`](LinearObjectWriter::finish).
    pub fn new() -> LinearObjectWriter {
        let mut writer = LinearObjectWriter {
            blocks: Vec::new(),
            current: vec![0u8; BLOCK_LENGTH],
            keys: Vec::new(),
            offsets: Vec::new(),
            num_objects_on_page: 0,
            space_left_on_block: BLOCK_LENGTH - OVERHEAD_PER_BLOCK,
            block_writing_position: 0,
            max_size: 0,
            blocks_generated: 0,
        };
        let dummy = [0u8; STORE_METADATA_SIZE];
        writer.write(0, &dummy);
        writer
    }

    /// Append one object.
    ///
    /// The object may span several blocks. Its table entry lives on the block
    /// where it starts.
    pub fn write(&mut self, key: u64, content: &[u8]) {
        let length = content.len();
        self.max_size = self.max_size.max(length);
        let mut written = 0;

        self.keys.push(key);
        self.offsets.push(self.block_writing_position as u16);
        self.num_objects_on_page += 1;
        self.space_left_on_block -= OVERHEAD_PER_OBJECT;

        loop {
            let to_write = self.space_left_on_block.min(length - written);
            self.current[self.block_writing_position..self.block_writing_position + to_write]
                .copy_from_slice(&content[written..written + to_write]);
            self.block_writing_position += to_write;
            self.space_left_on_block -= to_write;
            written += to_write;

            if self.space_left_on_block <= OVERHEAD_PER_OBJECT {
                self.write_table(self.space_left_on_block as u8);
            }
            if written >= length {
                break;
            }
        }
    }

    /// Flush the current block's table and start a fresh block.
    fn write_table(&mut self, empty_space: u8) {
        BlockStorage::write_table(
            &mut self.current,
            self.num_objects_on_page as u16,
            empty_space,
            &self.keys,
            &self.offsets,
        );
        self.blocks.extend_from_slice(&self.current);
        self.blocks_generated += 1;

        self.current = vec![0u8; BLOCK_LENGTH];
        self.keys.clear();
        self.offsets.clear();
        self.num_objects_on_page = 0;
        self.block_writing_position = 0;
        self.space_left_on_block = BLOCK_LENGTH - OVERHEAD_PER_BLOCK;
    }

    /// Close the store and return the file bytes.
    ///
    /// When the current block still has room, a zero-length terminator object
    /// with key 0 marks the end of the last real object. Block 0's header is
    /// patched with the real block count, max size, and store type.
    pub fn finish(mut self, type_: u16) -> Vec<u8> {
        if self.space_left_on_block <= 128 {
            let empty = self.space_left_on_block as u8;
            self.write_table(empty);
        } else {
            self.write(0, &[]);
            self.write_table(42);
        }

        let metadata = StoreMetadata {
            type_,
            num_blocks: self.blocks_generated as u64,
            max_size: self.max_size as u64,
        };
        self.blocks[0..STORE_METADATA_SIZE].copy_from_slice(&metadata.to_bytes());
        self.blocks
    }
}
