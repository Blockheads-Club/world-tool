//! Library, import and export.

mod common;

use bhw_core::{
    Choice, ConflictKind, ErrorCode, ExportFormat, ExportRequest, ImportKind, ImportRequest, ImportStatus, Origin,
    ReadRequest,
};
use common::*;

fn bhworld(env: &Env, file: &str, id: &str, name: &str) -> String {
    let path = env.path(file);
    let w = client_world(id, name);
    let info = bhw_world::ExportInfo {
        name: Some(name.into()),
        game_version: Some("1.7.5".into()),
        source: bhw_world::Source { kind: bhw_world::SourceKind::BlockheadsPc, device_name: Some("PC".into()) },
        created_at: None,
    };
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    bhw_world::bhworld::save_with_manifest(&path, &w, bhw_world::Manifest::new(id, &info), None).unwrap();
    path.display().to_string()
}

#[test]
fn import_with_choices() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    let one = bhworld(&env, "in/One.bhworld", A, "One");
    let server = env.path("in/server");
    bhw_save::save::write_dir_files(&server, &server_files(B, "Srv")).unwrap();
    let client = env.path("in/client");
    bhw_save::save::write_dir_files(&client, &client_files("not-an-id", "Folder")).unwrap();
    let bad = env.path("in/bad.bhworld");
    write(&bad, b"not a zip");
    let txt = env.path("in/notes.txt");
    write(&txt, b"x");
    let paths: Vec<String> = [
        one.clone(),
        server.display().to_string(),
        client.display().to_string(),
        bad.display().to_string(),
        txt.display().to_string(),
    ]
    .into();
    let items = core.inspect_import(&paths).unwrap();
    let k: Vec<_> = items.iter().map(|i| (i.kind, i.status)).collect();
    assert_eq!(
        k,
        [
            (ImportKind::Bhworld, ImportStatus::New),
            (ImportKind::Server, ImportStatus::New),
            (ImportKind::Server, ImportStatus::New),
            (ImportKind::Invalid, ImportStatus::Invalid),
            (ImportKind::Invalid, ImportStatus::Invalid),
        ]
    );
    assert_eq!(items[0].game_version.as_deref(), Some("1.7.5"));
    let added = core.import_worlds(&ImportRequest { choices: Default::default(), paths: paths.clone() }).unwrap();
    assert_eq!(added.len(), 3);
    let srv = added.iter().find(|w| w.base.name == "Srv").unwrap();
    assert_eq!((srv.source_detail.as_deref(), srv.origin, srv.world_id.as_str()), (Some("server"), Origin::File, B));
    assert_eq!(added[0].origin, Origin::Pc, "origin of a .bhworld comes from its manifest");
    let folder = added.iter().find(|w| w.base.name == "Folder").unwrap();
    assert_eq!(folder.world_id.len(), 32);

    // Again: everything clashes now.
    let items = core.inspect_import(&paths[..2]).unwrap();
    assert_eq!(items[0].conflict, Some(ConflictKind::SameId));
    assert_eq!(items[0].existing.as_deref(), Some("One"));
    let renamed = bhworld(&env, "in2/Other.bhworld", C, "srv");
    let items = core.inspect_import(std::slice::from_ref(&renamed)).unwrap();
    assert_eq!((items[0].conflict, items[0].existing.as_deref()), (Some(ConflictKind::SameName), Some("Srv")));
    let choices =
        [(one.clone(), Choice::KeepBoth), (paths[1].clone(), Choice::Replace), (renamed.clone(), Choice::Skip)]
            .into_iter()
            .collect();
    let added = core
        .import_worlds(&ImportRequest { choices, paths: vec![one.clone(), paths[1].clone(), renamed.clone()] })
        .unwrap();
    assert_eq!(added.len(), 2);
    assert_eq!(added[0].base.name, "One (2)");
    assert_ne!(added[0].world_id, A);
    let lib = core.lib().unwrap().list().unwrap();
    assert_eq!(lib.len(), 4);
    assert_eq!(lib.iter().filter(|w| w.base.name == "Srv").count(), 1);
    // Keep both for a same-name clash keeps the id.
    let choices = [(renamed.clone(), Choice::KeepBoth)].into_iter().collect();
    let added = core.import_worlds(&ImportRequest { choices, paths: vec![renamed] }).unwrap();
    assert_eq!((added[0].base.name.as_str(), added[0].world_id.as_str()), ("srv (2)", C));
    assert_eq!(core.list_activity()[0].kind, bhw_core::ActivityKind::Import);
}

#[test]
fn library_rename_duplicate_delete_and_index() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    let one = bhworld(&env, "in/One.bhworld", A, "One");
    core.import_worlds(&ImportRequest { choices: Default::default(), paths: vec![one] }).unwrap();
    let lib = core.lib().unwrap();
    let w = lib.list().unwrap().remove(0);
    let added = w.added;
    // The index is reused while the file is unchanged.
    assert_eq!(lib.list().unwrap()[0], w);

    let r = lib.rename(&w.base.id, "  Two ").unwrap();
    assert_eq!((r.base.name.as_str(), r.added, r.world_id.as_str()), ("Two", added, A));
    assert!(lib.path(&w.base.id).is_err());
    let d = lib.duplicate(&r.base.id).unwrap();
    assert_eq!(d.base.name, "Two (2)");
    assert_ne!(d.world_id, A);
    let opened = lib.open_world(&d.base.id).unwrap();
    assert_eq!(bhw_core::worlds::inspect(&opened.world, None).unwrap().name, "Two (2)");
    assert_eq!(lib.list().unwrap().len(), 2);
    lib.delete(&d.base.id).unwrap();
    assert_eq!(lib.delete(&d.base.id).unwrap_err().code, ErrorCode::NotFound);
    for bad in ["", ".hidden", "a/b", "..\\x"] {
        assert_eq!(lib.path(bad).unwrap_err().code, ErrorCode::NotFound);
    }

    // A damaged file and one added by hand are listed; a broken index is rebuilt.
    let dir = lib.inner.worlds_dir();
    std::fs::write(dir.join("broken.bhworld"), b"junk").unwrap();
    std::fs::copy(lib.path(&r.base.id).unwrap(), dir.join("copy.bhworld")).unwrap();
    std::fs::write(dir.join(".bhwt-index.json"), b"{").unwrap();
    let all = lib.list().unwrap();
    let broken = all.iter().find(|w| w.base.id == "broken.bhworld").unwrap();
    assert_eq!((broken.base.name.as_str(), broken.world_id.as_str()), ("broken", ""));
    let copy = all.iter().find(|w| w.base.id == "copy.bhworld").unwrap();
    assert_eq!((copy.base.name.as_str(), copy.origin, copy.source.as_str()), ("Two", Origin::Pc, "copy.bhworld"));
    // Broken files are not offered as a transfer source.
    let offered = core.source_worlds(&bhw_core::Source::Library).unwrap();
    assert_eq!(offered.len(), 2);
}

#[test]
fn export_formats() {
    let env = Env::new(&PhoneData::new(&[(A, "Sunny Isle"), (B, "Other")]));
    let core = env.core();
    let r = core.read("op", &ReadRequest { device_id: "S1".into(), password: String::new() }, &mut |_| {}).unwrap();
    let out = env.path("out");
    let req = ExportRequest {
        read_id: Some(r.read_id.clone()),
        world_ids: vec![A.into()],
        format: ExportFormat::Bhworld,
        folder: out.display().to_string(),
    };
    let (ev, mut on) = events();
    let res = core.export_worlds("op", &req, &mut on).unwrap();
    assert_eq!(res.saved.len(), 1);
    assert_eq!(res.files[0].name, "Sunny-Isle.bhworld");
    assert!(res.files[0].size > 0);
    assert_eq!(ev.lock().unwrap()[0].stage, bhw_core::Stage::Exporting);
    // Library worlds as server folders; a second .bhworld export gets a free name.
    let id = res.saved[0].base.id.clone();
    let req = ExportRequest {
        read_id: None,
        world_ids: vec![id.clone()],
        format: ExportFormat::Server,
        folder: req.folder.clone(),
    };
    let res = core.export_worlds("op", &req, &mut |_| {}).unwrap();
    assert_eq!(res.files[0].name, A);
    assert!(out.join(A).join("worldv2").is_file());
    let e = core.export_worlds("op", &req, &mut |_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::Io);
    let req = ExportRequest { format: ExportFormat::Bhworld, ..req };
    core.export_worlds("op", &req, &mut |_| {}).unwrap();
    let res = core.export_worlds("op", &req, &mut |_| {}).unwrap();
    assert_eq!(res.files[0].name, "Sunny-Isle (3).bhworld");
    assert_eq!(core.list_activity()[0].kind, bhw_core::ActivityKind::Export);
    // Free space of a folder that does not exist yet (its parent's disk).
    assert!(core.free_space(&out.join("new/deeper").display().to_string()).unwrap() > 0);
    assert_eq!(core.free_space("no-such-relative-folder").unwrap_err().code, ErrorCode::NotFound);
}
