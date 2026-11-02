use super::*;
use crate::extra::tests::{cell, folder, message, temp_pst, text};

const ROLES: [UnicodePstSpecialFolder; 5] = [
    UnicodePstSpecialFolder::DeletedItems,
    UnicodePstSpecialFolder::SentItems,
    UnicodePstSpecialFolder::Outbox,
    UnicodePstSpecialFolder::Drafts,
    UnicodePstSpecialFolder::JunkEmail,
];

fn outlook_pst(label: &str) -> std::path::PathBuf {
    let path = temp_pst(label);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/Empty.pst"),
        &path,
    )
    .unwrap();
    path
}

fn open_store(path: &Path) -> Rc<UnicodeStore> {
    UnicodeStore::read(Rc::new(UnicodePstFile::open(path).unwrap())).unwrap()
}

#[test]
fn a_folder_is_made_with_its_container_class() {
    let path = temp_pst("container-class");
    drop(UnicodePstFile::create(&path, &message("First", "<first@example.com>", &[])).unwrap());
    drop(UnicodePstFile::create_folder(&path, &["Plans", "Dates"], "IPF.Appointment").unwrap());
    // A folder already there keeps its own.
    drop(UnicodePstFile::create_folder(&path, &["Inbox"], "IPF.Contact").unwrap());
    assert_eq!(
        UnicodePstFile::create_folder(&path, &["Other"], "")
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );

    let store = open_store(&path);
    let class = |names: &[&str]| text(folder(&store, names).1.properties().get(0x3613));
    assert_eq!(class(&["Plans"]), "IPF.Note");
    assert_eq!(class(&["Plans", "Dates"]), "IPF.Appointment");
    assert_eq!(class(&["Inbox"]), "IPF.Note");
    let (dates, _) = folder(&store, &["Plans", "Dates"]);
    let (_, plans) = folder(&store, &["Plans"]);
    let hierarchy = plans.hierarchy_table().unwrap();
    if let Some(value) = cell(hierarchy.as_ref(), u32::from(dates), 0x3613) {
        assert_eq!(text(Some(&value)), "IPF.Appointment");
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn special_folders_are_recorded_and_found() {
    let path = temp_pst("special-folders");
    drop(UnicodePstFile::create(&path, &message("First", "<first@example.com>", &[])).unwrap());
    // Outlook's empty file, which the writer starts from, records only Deleted Items.
    for role in ROLES {
        let found = UnicodePstFile::special_folder(&path, role).unwrap();
        if role == UnicodePstSpecialFolder::DeletedItems {
            assert_eq!(found, Some(vec!["Deleted Items".to_string()]));
        } else {
            assert_eq!(found, None, "{role:?}");
        }
    }
    for role in ROLES {
        drop(UnicodePstFile::set_special_folder(&path, role, &[role.default_name()]).unwrap());
    }
    // One in a folder of its own, and recorded again in place of the first.
    drop(
        UnicodePstFile::set_special_folder(
            &path,
            UnicodePstSpecialFolder::JunkEmail,
            &["Inbox", "Spam"],
        )
        .unwrap(),
    );
    for role in ROLES {
        let expected: Vec<String> = match role {
            UnicodePstSpecialFolder::JunkEmail => vec!["Inbox".into(), "Spam".into()],
            role => vec![role.default_name().into()],
        };
        assert_eq!(
            UnicodePstFile::special_folder(&path, role).unwrap(),
            Some(expected),
            "{role:?}"
        );
    }

    let store = open_store(&path);
    let record_key = *store.properties().record_key().unwrap().record_key();
    let entry_id = |names: &[&str]| {
        Vec::try_from(&EntryId::new(
            StoreRecordKey::new(record_key),
            folder(&store, names).0,
        ))
        .unwrap()
    };
    for (id, name) in [
        (0x35E3, "Deleted Items"),
        (0x35E4, "Sent Items"),
        (0x35E2, "Outbox"),
    ] {
        assert!(
            matches!(store.properties().get(id), Some(PropertyValue::Binary(b)) if b.buffer() == entry_id(&[name]))
        );
    }
    let top = store
        .properties()
        .ipm_sub_tree_entry_id()
        .unwrap()
        .node_id();
    let (inbox, _) = folder(&store, &["Inbox"]);
    for node in [NID_ROOT_FOLDER, top, inbox] {
        let f = store
            .open_folder(&store.properties().make_entry_id(node).unwrap())
            .unwrap();
        let p = f.properties();
        assert!(
            matches!(p.get(0x36D7), Some(PropertyValue::Binary(b)) if b.buffer() == entry_id(&["Drafts"]))
        );
        match p.get(0x36D8) {
            Some(PropertyValue::MultipleBinary(values)) => {
                assert_eq!(values.len(), 5);
                assert_eq!(values[4].buffer(), entry_id(&["Inbox", "Spam"]));
            }
            value => panic!("{value:?}"),
        }
    }
    // The file still reads and takes mail.
    drop(store);
    drop(
        UnicodePstFile::append_in_folder(
            &path,
            &["Sent Items"],
            &message("Sent", "<sent@example.com>", &[]),
        )
        .unwrap(),
    );
    let store = open_store(&path);
    assert!(matches!(
        folder(&store, &["Sent Items"]).1.properties().get(0x3602),
        Some(PropertyValue::Integer32(1))
    ));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn outlooks_special_folders_are_kept() {
    let path = outlook_pst("special-outlook-kept");
    let before: Vec<_> = ROLES
        .iter()
        .map(|role| UnicodePstFile::special_folder(&path, *role).unwrap())
        .collect();
    assert_eq!(before[0], Some(vec!["Deleted Items".to_string()]));
    let junk_before = {
        let store = open_store(&path);
        let top = store
            .properties()
            .ipm_sub_tree_entry_id()
            .unwrap()
            .node_id();
        let f = store
            .open_folder(&store.properties().make_entry_id(top).unwrap())
            .unwrap();
        f.properties().get(0x36D8).cloned()
    };
    drop(
        UnicodePstFile::set_special_folder(&path, UnicodePstSpecialFolder::JunkEmail, &["Spam"])
            .unwrap(),
    );
    for (role, before) in ROLES.iter().zip(before) {
        let now = UnicodePstFile::special_folder(&path, *role).unwrap();
        if *role == UnicodePstSpecialFolder::JunkEmail {
            assert_eq!(now, Some(vec!["Spam".to_string()]));
        } else {
            assert_eq!(now, before, "{role:?}");
        }
    }
    // The other values of PidTagAdditionalRenEntryIds stay as they were.
    let store = open_store(&path);
    let top = store
        .properties()
        .ipm_sub_tree_entry_id()
        .unwrap()
        .node_id();
    let f = store
        .open_folder(&store.properties().make_entry_id(top).unwrap())
        .unwrap();
    if let (Some(PropertyValue::MultipleBinary(old)), Some(PropertyValue::MultipleBinary(new))) =
        (junk_before, f.properties().get(0x36D8))
    {
        for (index, value) in old.iter().enumerate().filter(|(index, _)| *index != 4) {
            assert_eq!(new[index].buffer(), value.buffer());
        }
    }
    std::fs::remove_file(path).unwrap();
}
