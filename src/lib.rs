//! A space-efficient static object store keyed by 64-bit integers.
//!
//! PaCHash maps distinct 64-bit keys to variable-length byte values. It packs
//! values fully into fixed 4096-byte blocks, then builds a small predecessor
//! index that costs about six bits per block. A point query reads one contiguous
//! block range and scans it.
//!
//! # Layout
//!
//! Each block stores object data from the front and a table of keys and offsets
//! from the back. An object that does not fit continues onto the next block.
//! Block 0 opens with a header ([`StoreMetadata`]) written as a pseudo object
//! with key 0. Key 0 is reserved.
//!
//! # Building and querying
//!
//! ```
//! use pachash::{PaCHashObjectStore, EliasFanoIndex};
//!
//! let items = vec![(10u64, b"alpha".to_vec()), (20u64, b"beta".to_vec())];
//! let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
//! let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();
//!
//! assert_eq!(&*store.query(10).unwrap(), b"alpha");
//! assert_eq!(&*store.query(20).unwrap(), b"beta");
//! assert!(store.query(30).is_none());
//! ```
//!
//! # Index variants
//!
//! Three indices back the same predecessor query and agree on every result:
//! [`EliasFanoIndex`] (default and smallest), [`UncompressedBitVectorIndex`],
//! and [`CompressedBitVectorIndex`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod block;
mod config;
mod hash;
mod index;
mod merge;
mod reader;
mod store;
mod writer;

pub use block::BlockStorage;
pub use config::{
    MetadataError, StoreMetadata, BLOCK_LENGTH, MAGIC, OVERHEAD_PER_BLOCK, OVERHEAD_PER_OBJECT,
    STORE_METADATA_SIZE, VERSION,
};
pub use hash::{ceillog2, fastrange64, key2bin, murmur_hash64, murmur_hash64_seeded};
pub use index::{CompressedBitVectorIndex, EliasFanoIndex, Index, UncompressedBitVectorIndex};
pub use merge::{merge, MergeError};
pub use reader::{LinearObjectReader, Object};
pub use store::{PaCHashObjectStore, StoreError};
pub use writer::LinearObjectWriter;
