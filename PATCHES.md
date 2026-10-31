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
2. **`PtypObject` reads** (`ltp/prop_type.rs`): `PropertyType::try_from` had no arm for `0x000D`,
   though the enum and the record reader have it, so an attached message's 0x3701 didn't parse.
   Test: `ltp::prop_type::tests`.
3. **A BTH record that doesn't parse is skipped** (`ltp/tree.rs`): each leaf record is read from
   its own fixed-size slot (key and value sizes from the BTH header), and one that fails is left
   out with a warning. The scan used to stop there, so every property after it was lost (an
   attachment kept 0x0E20 and 0x3001 and lost its method). A BTH that yields more than 2^24
   records is an error. Test: `ltp::tree::tests`.
4. **The object an attachment holds is looked up in its own subnode tree**
   (`messaging/attachment.rs`, `AttachmentInner::object_node`), where MS-PST keeps it, then in the
   message's, where this crate looked; for attached messages and OLE storage alike.
5. **An attached message is read once the file's lock is let go** (`messaging/attachment.rs`):
   `read_embedded` locks the file again, and `std::sync::Mutex` isn't re-entrant, so the first
   attached message hung the reader for good.
6. **A by-value attachment with no value reads as a zero-byte file** (`messaging/attachment.rs`):
   such a file is kept as 0x3701 with HNID 0, which reads as `PtypNull`, and was an error.
7. **`UnicodeAttachment::embedded_message()` and `AnsiAttachment::embedded_message()`** hand back
   the attached message as the file's own message type (kept beside the `Rc<dyn Message>` in
   `AttachmentData::Message`), so its own attachments read with `UnicodeAttachment::read`.

Fixes 1 and 4 to 7 are also proved on whole files in MailCraft's
`crates/net/src/data_files/pst/tests/reader.rs`: files made with `outlook-pst-rw` and changed
byte by byte into an attachment that holds a message, a zero-byte file, a data tree that says
less than it holds and one that points at itself, each read on a thread with a time limit.
