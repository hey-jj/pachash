# pachash

A space-efficient static object store keyed by 64-bit integers.

PaCHash maps distinct 64-bit keys to variable-length byte values. It packs
values fully into fixed 4096-byte blocks, then builds a small predecessor index
that costs about six bits per block. A point query reads one contiguous block
range and scans it.

## Install

```toml
[dependencies]
pachash = "0.1"
```

## Use

```rust
use pachash::{PaCHashObjectStore, EliasFanoIndex};

let items = vec![(10u64, b"alpha".to_vec()), (20u64, b"beta".to_vec())];
let bytes = PaCHashObjectStore::<EliasFanoIndex>::write_to_file(items).unwrap();
let store = PaCHashObjectStore::<EliasFanoIndex>::build_index(8, bytes).unwrap();

assert_eq!(store.query(10).unwrap().value, b"alpha");
assert!(store.query(30).is_none());
```

Keys are 64-bit. Hashing an application key to 64 bits is the caller's job. Key 0
is reserved for the file header. The string helpers `write_to_file_strings` and
`query_string` hash with MurmurHash64A.

## Layout

A store is a sequence of 4096-byte blocks. Object data grows from the front of
each block. A table of keys and start offsets grows from the back. An object that
does not fit continues onto the next block, so a query may read a short range and
stitch fragments. Block 0 opens with a 56-byte header written as a pseudo object
with key 0.

## Index variants

Three predecessor indices back the same query and agree on every result:

- `EliasFanoIndex`, the default and smallest.
- `UncompressedBitVectorIndex`, a plain bit vector with rank and select.
- `CompressedBitVectorIndex`, the same layout with the compressed-path math.

## Bin multiplier

The `a` parameter to `build_index` sets `num_bins = num_blocks * a`. A larger `a`
narrows each query's block range at the cost of a larger index. It does not
change the stored bytes.

## License

Licensed under the [MIT license](LICENSE).
