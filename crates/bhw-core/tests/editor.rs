//! The world editor service: opening library worlds, views, edits, history and saving.

mod common;

use bhw_content::fixtures::world_env;
use bhw_core::editor::EditRequest;
use bhw_core::{Core, ErrorCode};
use bhw_editor::{Brush, Layer, Match, OpKind, Rect};
use bhw_lmdb::Layout;
use common::*;

const ID: &str = "0123456789abcdef0123456789abcdef";
const Y: u32 = 512;

/// Puts the synthetic editor world into the library; returns its library id.
fn add_world(env: &Env, core: &Core) -> String {
    let mut files = bhw_save::FileMap::new();
    files.insert("world_db/data.mdb".into(), bhw_lmdb::write_env(&world_env(), Layout::Bits32).unwrap());
    files.insert("map/default/0_16.png".into(), b"png".to_vec());
    let w = bhw_save::world::from_file_map(ID, files).unwrap();
    let path = env.path("in/Test.bhworld");
    let info = bhw_world::ExportInfo {
        name: Some("TEST".into()),
        game_version: Some("1.7.6".into()),
        source: bhw_world::Source { kind: bhw_world::SourceKind::BlockheadsPc, device_name: None },
        created_at: None,
    };
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    bhw_world::bhworld::save_with_manifest(&path, &w, bhw_world::Manifest::new(ID, &info), None).unwrap();
    let added = core
        .import_worlds(&bhw_core::ImportRequest {
            choices: Default::default(),
            paths: vec![path.display().to_string()],
        })
        .unwrap();
    added[0].base.id.clone()
}

#[test]
fn open_view_edit_save() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    let lib_id = add_world(&env, &core);
    let w = core.editor_open(&lib_id).unwrap();
    assert_eq!((w.width, w.height, w.name.as_str()), (128, 1024, "TEST"));
    assert_eq!(w.blocks, vec![[0, 16], [1, 16]]);
    assert_eq!(w.spawn, Some([40, 520]));
    assert_eq!(w.game_version.as_deref(), Some("1.7.6"));
    assert!(!w.server && w.expert && w.damaged_blocks == 0);
    assert_eq!(w.blockheads[0].name, "ADA");
    assert_eq!(w.blockheads[0].inventory.as_ref().unwrap()[0], [4, 2]);
    let s = w.session.clone();

    let r = core.editor_region(&s, 0, 16, 2, 17).unwrap();
    assert_eq!(r.len(), 2 * (4 + 4096));
    let ov = core.editor_overview(&s, 4).unwrap();
    assert_eq!(
        (&ov[..4], &ov[4..8], ov.len()),
        (&32u32.to_le_bytes()[..], &256u32.to_le_bytes()[..], 8 + 32 * 256 * 4)
    );
    let t = core.editor_tile(&s, 3, 520).unwrap();
    assert!(t.stored && t.fg == 1 && !t.protected);
    assert_eq!(t.objects.len(), 1);
    assert_eq!(t.objects[0].kind_name.as_deref(), Some("chest"));
    assert!(!core.editor_tile(&s, 100, 5).unwrap().stored);
    assert!(core.editor_catalog().tiles.len() == 78);
    assert_eq!(core.editor_atlas("tiles").unwrap(), None);
    assert_eq!(core.editor_atlas("bogus").unwrap_err().code, ErrorCode::NotFound);

    let brush = Brush::front(6);
    let edits = vec![
        EditRequest::Paint { kind: OpKind::Pencil, points: vec![[1, Y]], brush, only_solid: false },
        EditRequest::Fill { rect: Rect { x: 2, y: Y, w: 2, h: 1 }, brush },
        EditRequest::Clear { rect: Rect { x: 4, y: Y, w: 1, h: 1 } },
        EditRequest::Replace {
            from: Match { layer: Layer::Front, tile: 1, contents: None },
            to: Brush::front(7),
            area: Some(Rect { x: 5, y: Y, w: 2, h: 1 }),
        },
        EditRequest::Copy { rect: Rect { x: 0, y: Y, w: 3, h: 2 } },
        EditRequest::Paste { x: 40, y: Y, skip_air: false, rotate: 1, flip: true },
        EditRequest::Cut { rect: Rect { x: 40, y: Y, w: 1, h: 1 } },
        EditRequest::MoveBlockhead { player: String::new(), id: 9, x: 10, y: 600 },
        EditRequest::SetStats { player: String::new(), id: 9, stats: [0.5, 0.5, 0.5, 0.5] },
        EditRequest::SetStack { player: String::new(), id: 9, slot: 1, item: 21, count: 3 },
        EditRequest::SetName { name: "Edited".into() },
        EditRequest::SetTimeOfDay { hour: 8.0 },
    ];
    let mut last = None;
    for e in edits {
        last = Some(core.editor_edit(&s, e).unwrap());
    }
    let st = last.unwrap().state;
    assert_eq!((st.history.len(), st.cursor, st.unsaved), (11, 11, 11));
    assert_eq!(st.clipboard, Some([1, 1]));
    assert_eq!(core.editor_blockheads(&s).unwrap()[0].x, 10);
    assert!(core.editor_changes(&s).unwrap().contains(&[1, Y]));
    let (found, n) = core.editor_find(&s, Match { layer: Layer::Front, tile: 6, contents: None }, None, 2).unwrap();
    assert_eq!((found.len(), n), (2, 5));
    // undo everything, then redo all
    assert_eq!(core.editor_history(&s, 0).unwrap().cursor, 0);
    assert_eq!(core.editor_history(&s, 11).unwrap().cursor, 11);
    assert!(core.editor_history(&s, 99).is_err());
    assert_eq!(core.editor_state(&s).unwrap().unsaved, 11);

    // save back into the library
    let saved = core.editor_save(&s, false).unwrap();
    assert_eq!(saved.base.id, lib_id);
    assert!(saved.edited.is_some());
    assert_eq!(saved.base.name, "EDITED");
    assert_eq!(core.editor_state(&s).unwrap().unsaved, 0);
    // reopening shows the edits
    let again = core.editor_open(&lib_id).unwrap();
    assert_eq!(again.name, "EDITED");
    assert_eq!(core.editor_tile(&again.session, 1, Y).unwrap().fg, 6);
    core.editor_close(&again.session);
    assert_eq!(core.editor_state(&again.session).unwrap_err().code, ErrorCode::NotFound);

    // save as a new world keeps the original
    core.editor_edit(
        &s,
        EditRequest::Paint { kind: OpKind::Pencil, points: vec![[0, Y]], brush: Brush::front(7), only_solid: false },
    )
    .unwrap();
    let new = core.editor_save(&s, true).unwrap();
    assert_ne!(new.base.id, lib_id);
    assert_ne!(new.world_id, ID);
    let n = core.editor_open(&new.base.id).unwrap();
    assert_eq!(core.editor_tile(&n.session, 0, Y).unwrap().fg, 7);
    assert_eq!(n.world_id, new.world_id);
    let o = core.editor_open(&lib_id).unwrap();
    assert_eq!(core.editor_tile(&o.session, 0, Y).unwrap().fg, 1);
    assert_eq!(core.lib().unwrap().list().unwrap().len(), 2);
}

#[test]
fn errors() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    assert_eq!(core.editor_open("nope.bhworld").unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(core.editor_region("x", 0, 0, 1, 1).unwrap_err().code, ErrorCode::NotFound);
    let lib_id = add_world(&env, &core);
    let s = core.editor_open(&lib_id).unwrap().session;
    let paste = EditRequest::Paste { x: 0, y: 0, skip_air: false, rotate: 0, flip: false };
    assert_eq!(core.editor_edit(&s, paste).unwrap_err().code, ErrorCode::NotAllowed);
    let bad = EditRequest::Fill { rect: Rect { x: 0, y: 0, w: 0, h: 0 }, brush: Brush::front(6) };
    assert_eq!(core.editor_edit(&s, bad).unwrap_err().code, ErrorCode::NotAllowed);
    assert!(core.editor_region(&s, 0, 0, 100, 100).is_ok(), "clamped to the world");
    assert!(core.editor_region(&s, 0, 0, 4, 32).is_ok());
    // a damaged block
    let e: bhw_core::AppError = bhw_editor::Error::Content(bhw_content::Error::Format("x".into())).into();
    assert_eq!(e.code, ErrorCode::WorldDamaged);
    let e: bhw_core::AppError =
        bhw_content::Error::Plist(plist::Value::from_reader(std::io::Cursor::new(b"<")).unwrap_err()).into();
    assert_eq!(e.code, ErrorCode::WorldDamaged);
}

#[test]
fn server_and_remote_worlds_and_atlas() {
    let env = Env::new(&PhoneData::new(&[]));
    let core = env.core();
    // server world
    let dir = env.path("in/server");
    bhw_save::save::write_dir_files(&dir, &server_files(B, "Srv")).unwrap();
    let added = core
        .import_worlds(&bhw_core::ImportRequest { choices: Default::default(), paths: vec![dir.display().to_string()] })
        .unwrap();
    let w = core.editor_open(&added[0].base.id).unwrap();
    assert!(w.blocks.contains(&[1, 1]));
    core.editor_edit(
        &w.session,
        EditRequest::Paint { kind: OpKind::Pencil, points: vec![[32, 32]], brush: Brush::front(7), only_solid: false },
    )
    .unwrap();
    core.editor_save(&w.session, false).unwrap();
    // a client's cache of a multiplayer world is refused
    let mut files = bhw_save::FileMap::new();
    files.insert(
        "world_db/data.mdb".into(),
        bhw_lmdb::write_env(&client_env(C, "Remote", true, true), Layout::Bits32).unwrap(),
    );
    let rw = bhw_save::world::from_file_map(C, files).unwrap();
    let info = bhw_world::ExportInfo {
        name: Some("Remote".into()),
        game_version: None,
        source: bhw_world::Source { kind: bhw_world::SourceKind::BlockheadsPc, device_name: None },
        created_at: None,
    };
    let rpath = env.path("Worlds/worlds/Remote.bhworld");
    std::fs::create_dir_all(rpath.parent().unwrap()).unwrap();
    bhw_world::bhworld::save_with_manifest(&rpath, &rw, bhw_world::Manifest::new(C, &info), None).unwrap();
    assert_eq!(core.editor_open("Remote.bhworld").unwrap_err().code, ErrorCode::NotAllowed);
    // a server world placed in the library folder by hand stays a server world (64-bit) when saved
    let sw = bhw_save::world::from_file_map(D, server_files(D, "Raw")).unwrap();
    let spath = env.path("Worlds/worlds/Raw.bhworld");
    bhw_world::bhworld::save_with_manifest(&spath, &sw, bhw_world::Manifest::new(D, &info), None).unwrap();
    let raw = core.editor_open("Raw.bhworld").unwrap();
    assert!(raw.server);
    core.editor_edit(
        &raw.session,
        EditRequest::Paint { kind: OpKind::Pencil, points: vec![[33, 33]], brush: Brush::front(7), only_solid: false },
    )
    .unwrap();
    core.editor_save(&raw.session, false).unwrap();
    let back = bhw_world::bhworld::open_bhworld(&spath, &Default::default()).unwrap();
    let db = back.world.get("world_db/data.mdb").unwrap().data.read_all().unwrap();
    assert_eq!(Layout::detect(&db), Some(Layout::Bits64));
    // game files present: the atlas comes from the cache
    let apk_path = env.path("dl/TheBlockheads.apk");
    write(&apk_path, &apk("1.7.6"));
    core.use_apk("op", &apk_path.display().to_string(), &mut |_| {}).unwrap();
    assert!(core.editor_atlas("tiles").unwrap().is_some());
    assert!(core.editor_atlas("items").unwrap().is_some());
}
