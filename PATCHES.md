# outlook-pst-rw for MailCraft

This branch, `craft/rw-1.2.14`, is Dong Shin's Outlook data file writer,
[outlook-pst-rw](https://crates.io/crates/outlook-pst-rw) 1.2.14 (the tag `outlook-pst-rw-v1.2.14`,
commit `4799f5d0`, of [dongs0104/outlook-pst-rs](https://github.com/dongs0104/outlook-pst-rs), MIT,
`LICENSE`), with the fixes below. That crate is itself a fork of Microsoft's reader,
[outlook-pst](https://crates.io/crates/outlook-pst), which MailCraft builds from this repository's
`craft/1.2.0` branch; the writer keeps its own copy of the reader's modules, so the reader's fixes
are carried over here as well. [MailCraft](https://github.com/ZIDAN44/mailcraft) builds the writer
through `[patch.crates-io]` in its `Cargo.toml` and in `fuzz/Cargo.toml`, pinned to a revision of
this branch; its `cargo xtask ci` checks that both pin the same one.

Only `crates/pst` changes. Each fix is its own commit, follows MailCraft's rules all the same (no
new panics, checked sizes), and is proved by the tests beside it (`cargo test -p outlook-pst-rw`).

To move to a newer release: fetch `dongs0104`'s tags
(`git remote add dongs0104 https://github.com/dongs0104/outlook-pst-rs.git`,
`git fetch dongs0104 --tags`), make `craft/rw-<version>` from the release's tag
(`outlook-pst-rw-v<version>`), cherry-pick this branch's commits onto it, dropping the fixes the
release already has, and pin MailCraft to it. Never rewrite a pushed branch: MailCraft's history
pins its revisions. Drop the fork once a release has all of the fixes.

## Fixes

Every changed line is marked `MailCraft:` in a comment, with a test beside it.

1. **Reads are capped** (`ndb/block.rs`, `ndb/read_write.rs`, `ltp/prop_context.rs`, `lib.rs`):
   - a data tree is read up to the size its XBLOCK declares (`DataTree::read_capped`), so blocks
     named twice can't make a value bigger than that;
   - `UnicodePstFile::set_max_value_size` (and the Ansi twin) sets a limit for a message's and an
     attachment's values (`PropertyContextReadWrite::read_property_capped`): a value declared
     bigger is an error before any of it is read;
   - a data tree is level 1 or 2 and a subnode tree level 0 or 1, and each block points one
     level down, so a tree that points back at itself is an error, not a stack overflow;
   - an intermediate block's entry count is checked in `u32`, and a block smaller than its header
     is an error, where both overflowed `u16`.

   Tests: `ndb::block::tests`.

Fix 1 is also proved on whole files in MailCraft's `crates/net/src/data_files/pst/tests/reader.rs`:
files made with `outlook-pst-rw` and changed byte by byte into a data tree that says less than it
holds and one that points at itself, each read on a thread with a time limit.
