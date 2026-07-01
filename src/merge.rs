//! Merge several stores into one.
//!
//! [`merge`] does a k-way merge of stores that are already sorted by key. Each
//! round it picks the smallest current key across all inputs, writes that
//! object, and advances that input. The result is one store sorted by key.

use crate::config::{MetadataError, StoreMetadata};
use crate::reader::LinearObjectReader;
use crate::writer::LinearObjectWriter;

/// Error from a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    /// An input file header was invalid.
    Metadata(MetadataError),
    /// Two inputs held the same key.
    KeyCollision(u64),
}

impl core::fmt::Display for MergeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MergeError::Metadata(e) => write!(f, "{e}"),
            MergeError::KeyCollision(k) => write!(f, "key collision on {k}"),
        }
    }
}

impl std::error::Error for MergeError {}

impl From<MetadataError> for MergeError {
    fn from(e: MetadataError) -> Self {
        MergeError::Metadata(e)
    }
}

/// Merge `inputs` into one store's bytes.
///
/// Inputs must be valid PaCHash stores sorted by key. Returns an error when any
/// header is invalid or when two inputs share a key.
pub fn merge(inputs: &[Vec<u8>]) -> Result<Vec<u8>, MergeError> {
    let mut readers: Vec<LinearObjectReader> = Vec::with_capacity(inputs.len());
    for input in inputs {
        readers.push(LinearObjectReader::new(input)?);
    }

    let mut writer = LinearObjectWriter::new();

    loop {
        let mut min_reader: Option<usize> = None;
        let mut min_key: u64 = u64::MAX;
        let mut seen_min = false;
        for (i, reader) in readers.iter().enumerate() {
            if !reader.has_current() {
                continue;
            }
            let key = reader.current_key();
            if key == min_key && seen_min {
                return Err(MergeError::KeyCollision(key));
            }
            if key < min_key {
                min_key = key;
                min_reader = Some(i);
                seen_min = true;
            }
        }

        let Some(idx) = min_reader else {
            break;
        };
        let value = readers[idx].current_value().to_vec();
        writer.write(min_key, &value);
        readers[idx].advance();
    }

    Ok(writer.finish(StoreMetadata::TYPE_PACHASH))
}
