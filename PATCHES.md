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

Every changed line is marked `MailCraft:` in a comment, with a test beside it. Fixes 1 to 8 are
the reader's, from `craft/1.2.0` (commits `560024b`, `4832e88`, `33c5231`, `647f6fd`, `38765ba`,
`b3a2ed1`), cherry-picked with their authors onto the writer's copy of the reader; 9 to 12 are
the writer's own.

1. **Reads are capped** (`ndb/block.rs`, `ndb/page.rs`, `ndb/read_write.rs`, `ltp/heap.rs`,
   `ltp/prop_context.rs`, `ltp/table_context.rs`, `messaging/message.rs`, `messaging/attachment.rs`,
   `messaging/store.rs`, `messaging/folder.rs`, `messaging/named_prop.rs`, `messaging/search.rs`,
   `lib.rs`):
   - every data tree is read up to the size its XBLOCK declares: `DataTree::reader` stops there,
     and `blocks()` and the reader's list of blocks stop naming blocks once that much is named,
     so blocks named over and over (an XXBLOCK's thousand entries naming one XBLOCK whose
     thousand name one block) can't make a value, a heap or a table's rows bigger than that;
   - `UnicodePstFile::set_max_value_size` (and the Ansi twin) sets a limit for the store's, a
     folder's, the name-to-id map's, a message's and an attachment's values
     (`PropertyContextReadWrite::read_property_capped`; nothing in the crate calls the uncapped
     `read_property` any more), a table's column
     kept in a subnode and its row matrix, `read_block` and the search update queue
     (`DataTree::read_capped`): one declared bigger is an error before any of it is read;
     MailCraft sets 256 MB. A heap that declares more than 2^16 blocks can hold is an error;
   - a data tree is level 1 or 2 and a subnode tree level 0 or 1, each block points one level
     down, and so does each node and block B-tree page (`RootBTreePage::find_entry`), so a tree
     that points back at itself is an error, not a stack overflow;
   - an intermediate block's entry count is checked in `u32`, and a block smaller than its header
     is an error, where both overflowed `u16`.

   Tests: `ndb::block::tests`.
2. **`PtypObject` reads** (`ltp/prop_type.rs`): `PropertyType::try_from` had no arm for `0x000D`,
   though the enum and the record reader have it, so an attached message's 0x3701 didn't parse.
   Test: `ltp::prop_type::tests`.
3. **A BTH record that doesn't parse is skipped** (`ltp/tree.rs`): each leaf record is read from
   its own fixed-size slot (key and value sizes from the BTH header), and one that fails is left
   out with a warning. The scan used to stop there, so every property after it was lost (an
   attachment kept 0x0E20 and 0x3001 and lost its method). A BTH that yields more records than
   its key size has keys (65,536 for a property context), or more than 2^24, is an error, and
   so is one whose index records, counted over all its levels (the header may declare 255),
   come to more than that: records that point back up the tree can't keep the walk going.
   Test: `ltp::tree::tests`.
4. **The object an attachment holds is looked up in its own subnode tree**
   (`messaging/attachment.rs`, `AttachmentInner::object_node`), where MS-PST keeps it, then in the
   message's, where this crate looked; for attached messages and OLE storage alike. An own tree
   that can't be read is passed over with a warning.
5. **An attached message is read once the file's lock is let go** (`messaging/attachment.rs`):
   `read_embedded` locks the file again, and `std::sync::Mutex` isn't re-entrant, so the first
   attached message hung the reader for good.
6. **A by-value attachment with no value reads as a zero-byte file** (`messaging/attachment.rs`):
   such a file is kept as 0x3701 with HNID 0, which reads as `PtypNull`, and was an error.
7. **`UnicodeAttachment::embedded_message()` and `AnsiAttachment::embedded_message()`** hand back
   the attached message as the file's own message type (kept beside the `Rc<dyn Message>` in
   `AttachmentData::Message`), so its own attachments read with `UnicodeAttachment::read`.
8. **The root folder's hierarchy table reads** (`messaging/store.rs`): upstream's fix
   (microsoft/outlook-pst-rs#55, commit `d0f9f001`) is already in outlook-pst-rw 1.2.14, so
   only its test comes over from `craft/1.2.0`. `Store::root_hierarchy_table` held the file's
   lock while `TableContextReadWrite::read` locked it again, so every call hung for good.
   Test: `messaging::store::tests` (fails on its time limit without the fix).
9. **The writer reads a file it adds to as the reader does** (`lib.rs`, `ltp/heap.rs`,
   `ndb/block.rs`; with the reader's limit in force since fix 12):
   - its own walks of a data tree (`data_tree_leaves`, `read_data_block`) read data blocks with
     the file's `bCryptMethod`, where they read them as if unencoded, take only XBLOCKs over
     leaves and XXBLOCKs over XBLOCKs, and stop naming leaves at the size the top block
     declares, like `DataTree::blocks`; a tree that names itself was a stack overflow;
   - its walk of the node and block B-trees (`BTreeLayout::read`) takes each page a level below
     the one that names it and each page once, where one page named twice was listed twice and
     pages that named more pages at their own level were walked 500^8 times;
   - a block size no block can have (over 8,192 bytes with its trailer) is an error in
     `DataTree::read` and `SubNodeTree::read`, where `block_size` asserted and the sum
     overflowed `u16`;
   - a heap allocation placed past its block is an error (`HeapNode::find_entry`,
     `writer_pages`), and so are a table row too short for its ID and a column past its row
     (`encode_table_row`, `patch_table_row`), an allocation map bit before the first map or
     past its own map (`mark_page_allocation`, `mark_block_allocation`), a row matrix with no
     blocks (`append_table_rows`) and an end of file before the first map: each one panicked
     or overflowed;
   - `internal_block_children` returns an error where it had `unreachable!`, and the row
     index reads lose their `unwrap`s.

   Tests: `existing_file_tests`, `ltp::heap::tests`.
10. **A permute- or cyclic-encoded file is added to** (`lib.rs`): files Outlook makes are
    permute-encoded (`bCryptMethod` 0x01) by default, and `append*`, `create_*_in_folder` and
    `set_store_display_name` refused every file that wasn't unencoded. Their new data blocks
    are now written encoded with the file's method (`BlockWriter`, `write_new_block`; cyclic
    is keyed on each block's ID), and only data blocks: XBLOCKs, XXBLOCKs, SLBLOCKs and
    SIBLOCKs never are (MS-PST 5.1, 5.2). Existing blocks are read decoded (fix 9). Files the
    writer creates stay unencoded.

    Test: `create_tests::appends_to_permute_and_cyclic_encoded_unicode_psts` adds to Outlook's
    own fixture as Outlook made it (permute) and re-encoded cyclic: a renamed store, a message
    with an 8 KB+ body and a 20,000-byte attachment, a batch of 40 that appends the contents
    table's rows in place, and a new nested folder, all read back with the crate's reader; no
    data block's raw bytes show the body's text.
11. **A damaged allocation map is not trusted** (`lib.rs`, `FileSpace::read`): new blocks and
    pages go where the file's allocation map marks space free, and the map was taken whenever
    the header called it valid, checking only each page's type byte. A map page now has to read
    as the reader reads one (type and its repeat, CRC, a zero signature, its own offset as its
    ID), and no space it marks free may hold a block the block B-tree lists, a page of either
    B-tree or a map page; otherwise the map is treated as invalid and new blocks go at the end
    of the file. A map with live blocks marked free put new blocks over them: the append
    succeeded and old mail no longer read. Outlook's own maps still pass, so their free space
    is still reused.

    Test: `create_tests::a_damaged_allocation_map_is_not_trusted` (Outlook's fixture after 60
    appends, its last map page marking live blocks free with a matching or a stale CRC; every
    message still reads after one more append).
12. **A file the writer adds to is read within a limit** (`lib.rs`, `open_to_add`): the writer
    reads the file it adds to through the reader's capped paths (fix 1), but the limit was
    `u64::MAX` and nothing set it, so a damaged file could still have it build a value, a
    table's rows or a column kept in a subnode of whatever size the file declared, up to 4 GiB
    from a few KB of blocks named over and over. `append*`, `create_*_in_folder` and
    `set_store_display_name` now open it with `ADD_MAX_VALUE_SIZE` (256 MB, MailCraft's own
    limit) set; anything declared bigger is an error before any of it is read.

    Test: `create_tests::a_file_added_to_is_read_within_the_limit` (with the limit lowered on
    the test's thread, an append to a folder whose tables hold more is an error and leaves the
    file as it was).
