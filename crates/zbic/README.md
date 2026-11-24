# zbic

A decoder for ZBIC, the compression of NSO segments from Switch firmware 22
on (NSO flags bit 7): zstd frames with the magic `ZBIC` (0x4349425A) whose
FSE table headers are written with binary interpolative coding.

It is the decoding half of [ruzstd](https://github.com/KillingSpark/zstd-rs)
0.9.1, as published on crates.io, under its MIT license (`LICENSE`, kept as
it came). What changed from it:

- Left out: the encoder, the dictionary builder, the xxhash checksum (the
  `hash` feature) and the crate's tests and corpus.
- `common/mod.rs`: the frame magic is ZBIC's.
- `fse/fse_decoder.rs`: an FSE table header is read by `src/bic.rs`, which
  is ours, instead of as zstd writes it. `bit_io/bit_reader.rs` loses the
  two methods only zstd's table reader used.
- `decoding/block_decoder.rs` and `decoding/sequence_execution.rs`: a
  block's literals, and the literals after its last sequence, count toward
  the block's size limit. ruzstd 0.9.1 checks neither, so a crafted block
  could decode past it.
- `lib.rs`: the crate's surface is `decode`, a segment's frames into a
  buffer of its size, plus the frame tests.
