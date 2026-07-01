//! On-disk constants and the file header.
//!
//! A store is a sequence of fixed 4096-byte blocks. Object data grows from the
//! front of each block. A small table grows from the back. The first object of
//! block 0 carries a [`StoreMetadata`] record with key 0.

/// Length of one on-disk block in bytes.
pub const BLOCK_LENGTH: usize = 4096;

/// Bytes the table spends per object: one 8-byte key plus one 2-byte offset.
pub const OVERHEAD_PER_OBJECT: usize = 8 + 2;

/// Bytes the table spends per block: a 2-byte object count plus a 1-byte
/// empty-page marker.
pub const OVERHEAD_PER_BLOCK: usize = 2 + 1;

/// Serialized size of [`StoreMetadata`].
///
/// This matches the byte image a C compiler produces for the equivalent struct
/// under LP64 alignment: `magic[32]`, `version` at 32, `type` at 34, two bytes
/// of padding, then two 8-byte fields at 40 and 48.
pub const STORE_METADATA_SIZE: usize = 56;

/// Exact 32 magic bytes at the start of every store file. The string is 31
/// characters plus a NUL, padded with zeros to 32.
pub const MAGIC: [u8; 32] = *b"Variable size object store file\0";

/// File-format version this crate reads and writes.
pub const VERSION: u8 = 1;

/// The file header, packed into the first object of block 0.
///
/// The on-disk image is little-endian with the field offsets described on
/// [`STORE_METADATA_SIZE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreMetadata {
    /// Store kind. See the `TYPE_*` associated constants.
    pub kind: u16,
    /// Number of blocks in the file.
    pub num_blocks: u64,
    /// Largest object length written to the file.
    pub max_size: u64,
}

impl StoreMetadata {
    /// PaCHash store.
    pub const TYPE_PACHASH: u16 = 1000;
    /// Separator store. The stored type adds the separator bit width.
    pub const TYPE_SEPARATOR: u16 = 2000;
    /// Cuckoo store.
    pub const TYPE_CUCKOO: u16 = 0;

    /// Serialize into the exact 56-byte on-disk image.
    pub fn to_bytes(&self) -> [u8; STORE_METADATA_SIZE] {
        let mut out = [0u8; STORE_METADATA_SIZE];
        out[0..32].copy_from_slice(&MAGIC);
        out[32] = VERSION;
        out[34..36].copy_from_slice(&self.kind.to_le_bytes());
        out[40..48].copy_from_slice(&self.num_blocks.to_le_bytes());
        out[48..56].copy_from_slice(&self.max_size.to_le_bytes());
        out
    }

    /// Parse from the first 56 bytes of block 0.
    ///
    /// Checks the magic bytes first, then the version. Returns an error when
    /// either does not match.
    pub fn from_bytes(data: &[u8]) -> Result<Self, MetadataError> {
        if data.len() < STORE_METADATA_SIZE {
            return Err(MetadataError::Truncated);
        }
        if data[0..32] != MAGIC {
            return Err(MetadataError::BadMagic);
        }
        let version = data[32];
        if version != VERSION {
            return Err(MetadataError::BadVersion(version));
        }
        let kind = u16::from_le_bytes([data[34], data[35]]);
        let num_blocks = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let max_size = u64::from_le_bytes(data[48..56].try_into().unwrap());
        Ok(StoreMetadata {
            kind,
            num_blocks,
            max_size,
        })
    }
}

/// Errors from reading a store header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    /// The file is shorter than the metadata or its declared block count.
    Truncated,
    /// The magic bytes do not match a store file.
    BadMagic,
    /// The file version is not supported. Holds the version found.
    BadVersion(u8),
}

impl core::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MetadataError::Truncated => write!(f, "file is too short to hold metadata"),
            MetadataError::BadMagic => {
                write!(
                    f,
                    "magic bytes do not match. Is this really an object store?"
                )
            }
            MetadataError::BadVersion(v) => write!(
                f,
                "loaded file is version {v} but this binary supports only version {VERSION}"
            ),
        }
    }
}

impl std::error::Error for MetadataError {}
