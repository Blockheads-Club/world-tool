mod common;

use bhw_lmdb::Layout;
use bhw_save::convert::{client_to_server_files, server_to_client_files, CLIENT_EXTRAS, SERVER_EXTRAS};
use bhw_save::identity::set_identity;
use bhw_save::preview::{render_blocks, tile_color, UNKNOWN, UNKNOWN_TILE};
use bhw_save::save::{
    detect_kind_files, env_from, get, is_world_id, put, read_dir_files, remove, write_dir_files, LIGHT_BLOCKS,
    SERVER_DB, WORLD_DB,
};
use bhw_save::*;
use common::*;
use plist::Value;

fn opts() -> ConvertOptions {
    ConvertOptions::default()
}

#[test]
fn client_summary() {
    let save = AnySave::from_files(CID, client_files(CID, "HOME")).unwrap();
    let mut gdb = GameDb::new();
    let s = summarize_any(&save, Some(&gdb));
    assert_eq!(s.kind, Some(SaveKind::Client));
    assert_eq!(s.id, CID);
    assert_eq!(s.name.as_deref(), Some("HOME"));
    assert_eq!(s.created, Some(1792486089));
    assert_eq!(s.last_saved, Some(1792577746));
    assert!(s.expert_mode && !s.custom_rules && !s.remote_game && !s.cloud_game);
    assert_eq!(s.world_width_macro, Some(4));
    assert_eq!(s.world_width_tiles(), Some(128));
    assert_eq!(s.seed, Some(1792486087));
    assert_eq!(s.save_version, Some(1100));
    assert_eq!(s.start_portal, Some((40, 35)));
    assert!(s.has_preview_image);
    assert_eq!(s.blockheads, vec![BlockheadInfo { owner: None, name: "ALAN".into(), unique_id: 5 }]);
    assert_eq!((s.block_count, s.dynamic_record_count, s.player_count), (3, 1, None));
    assert_eq!(s.registered_in_game_db, Some(false));
    assert!(s.size_on_disk > 0);
    assert!(s.problems.is_empty());
    assert_eq!(save.world_db().named.len(), 3);
    gdb.register(CID, save.worldv2().unwrap());
    assert_eq!(summarize_any(&save, Some(&gdb)).registered_in_game_db, Some(true));
    assert_eq!(summarize_any(&save, None).registered_in_game_db, None);
}

#[test]
fn client_summary_without_record() {
    let mut env = client_env(CID, "HOME");
    let rec = get(&env, "main", "worldv2").unwrap().to_vec();
    remove(&mut env, "main", b"worldv2");
    // an undecodable blockheads record is reported, not fatal
    put(&mut env, "main", b"x_blockheads", b"garbage".to_vec());
    let mut files = FileMap::new();
    files.insert(WORLD_DB.into(), bhw_lmdb::write_env(&env, Layout::Bits32).unwrap());
    let save = ClientSave::from_files(CID, files).unwrap();
    let s = summary::summarize_client(&save, None);
    assert_eq!(s.name, None);
    assert_eq!(s.problems.len(), 2, "{:?}", s.problems);
    let mut gdb = GameDb::new();
    gdb.register(CID, &rec);
    let s = summary::summarize_client(&save, Some(&gdb));
    assert_eq!(s.name.as_deref(), Some("HOME"));
    assert_eq!(s.registered_in_game_db, Some(true));
}

#[test]
fn bad_and_remote_records() {
    let mut env = client_env(CID, "HOME");
    put(&mut env, "main", b"worldv2", b"not a plist".to_vec());
    let mut files = FileMap::new();
    files.insert(WORLD_DB.into(), bhw_lmdb::write_env(&env, Layout::Bits32).unwrap());
    let s = summarize_any(&AnySave::from_files(CID, files).unwrap(), None);
    assert!(s.problems[0].starts_with("worldv2:"), "{:?}", s.problems);

    let mut r = record(SID, "TEST");
    r.insert("remoteGame".into(), Value::Boolean(true));
    r.insert("host".into(), Value::String("127.0.0.1".into()));
    r.insert("port".into(), Value::String("15151".into()));
    r.insert("connectionDate".into(), date("2026-10-25T21:07:06Z"));
    r.insert("cloudGame".into(), Value::Boolean(true));
    r.insert("worldWidthMacro".into(), Value::Real(512.0));
    r.insert("randomSeed".into(), Value::String(" 12 ".into()));
    r.insert("saveVersion".into(), Value::Boolean(true));
    let mut env = client_env(SID, "TEST");
    put(&mut env, "main", b"worldv2", plistio::to_xml(&Value::Dictionary(r.clone()), plistio::Dialect::Apple).unwrap());
    put(&mut env, "main", b"local_blockheads", plistio::gzip(&blockheads(&[("SCOTT", 9)])));
    let mut files = FileMap::new();
    files.insert(WORLD_DB.into(), bhw_lmdb::write_env(&env, Layout::Bits32).unwrap());
    let s = summarize_any(&AnySave::from_files(SID, files.clone()).unwrap(), None);
    assert!(s.remote_game && s.cloud_game);
    assert_eq!(s.remote_host.as_deref(), Some("127.0.0.1:15151"));
    assert_eq!(s.last_connected, Some(1792962426));
    assert_eq!((s.world_width_macro, s.seed, s.save_version), (Some(512), Some(12), None));
    assert!(s.blockheads.iter().any(|b| b.owner.as_deref() == Some("local") && b.name == "SCOTT"));
    let e = client_to_server_files(&files, &opts()).unwrap_err();
    assert!(matches!(e, Error::Unsupported(_)), "{e}");

    r.remove("port");
    put(&mut env, "main", b"worldv2", plistio::to_xml(&Value::Dictionary(r), plistio::Dialect::Apple).unwrap());
    files.insert(WORLD_DB.into(), bhw_lmdb::write_env(&env, Layout::Bits32).unwrap());
    let s = summarize_any(&AnySave::from_files(SID, files).unwrap(), None);
    assert_eq!(s.remote_host.as_deref(), Some("127.0.0.1"));
}

#[test]
fn server_summary() {
    let save = AnySave::from_files(SID, server_files(SID, "SRV")).unwrap();
    let s = summarize_any(&save, None);
    assert_eq!(s.kind, Some(SaveKind::Server));
    assert_eq!(s.name.as_deref(), Some("SRV"));
    assert!(s.custom_rules);
    assert_eq!(s.player_count, Some(1));
    assert_eq!(s.blockheads.len(), 2);
    assert!(s.blockheads.iter().all(|b| b.owner.as_deref() == Some("p1")));
    assert!(!s.has_preview_image);

    // never run: only the worldv2 file
    let mut env = server_env(SID, "SRV");
    remove(&mut env, "main", b"worldv2");
    let mut files = server_files(SID, "SRV");
    files.insert(WORLD_DB.into(), bhw_lmdb::write_env(&env, Layout::Bits64).unwrap());
    files.remove(SERVER_DB);
    let save = AnySave::from_files(SID, files.clone()).unwrap();
    let s = summarize_any(&save, None);
    assert_eq!(s.name.as_deref(), Some("SRV"));
    assert_eq!(s.seed, None);
    assert_eq!(s.player_count, Some(0));
    assert!(save.worldv2().is_some());
    let e = server_to_client_files(&files, &opts()).unwrap_err();
    assert!(e.to_string().contains("never saved"), "{e}");
    files.remove("worldv2");
    let s = summarize_any(&AnySave::from_files(SID, files).unwrap(), None);
    assert_eq!(s.problems, vec!["no worldv2 record or file".to_string()]);
}

#[test]
fn kinds_and_errors() {
    assert_eq!(detect_kind_files(&client_files(CID, "A")), Some(SaveKind::Client));
    assert_eq!(detect_kind_files(&server_files(SID, "A")), Some(SaveKind::Server));
    let mut lb = FileMap::new();
    lb.insert("lightBlocks/data.mdb".into(), vec![]);
    assert_eq!(detect_kind_files(&lb), Some(SaveKind::Server));
    assert_eq!(detect_kind_files(&FileMap::new()), None);
    assert!(matches!(AnySave::from_files("x", FileMap::new()), Err(Error::Format(_))));
    let mut only_map = FileMap::new();
    only_map.insert("world_db/lock.mdb".into(), vec![]);
    assert!(ClientSave::from_files("x", only_map.clone()).unwrap_err().to_string().contains("no world_db"));
    assert!(ServerSave::from_files("x", only_map).unwrap_err().to_string().contains("no world_db"));
    let mut bad = FileMap::new();
    bad.insert(WORLD_DB.into(), vec![0; 100]);
    assert!(matches!(ClientSave::from_files("x", bad), Err(Error::Lmdb(_))));
    assert!(is_world_id(CID) && !is_world_id("ABC") && !is_world_id(&"g".repeat(32)));
}

#[test]
fn folders() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(CID);
    let mut files = client_files(CID, "HOME");
    files.insert("map/empty/".into(), vec![]);
    write_dir_files(&dir, &files).unwrap();
    assert!(dir.join("map/empty").is_dir());
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.join("world_db"), dir.join("link")).unwrap();
    let back = read_dir_files(&dir).unwrap();
    assert!(!back.contains_key("world_db/lock.mdb") && !back.contains_key("link"));
    assert_eq!(back.len(), 2);
    assert_eq!(SaveKind::detect(&dir), Some(SaveKind::Client));
    assert_eq!(SaveKind::detect(tmp.path()), None);
    let s = summarize(&dir, None).unwrap();
    assert_eq!(s.dir_name, CID);
    assert!(ClientSave::open(&dir).is_ok());
    assert!(matches!(summarize(&tmp.path().join("missing"), None), Err(Error::Io { .. })));
    assert!(matches!(AnySave::open(tmp.path()), Err(Error::Format(_))));

    let sdir = tmp.path().join(SID);
    write_dir_files(&sdir, &server_files(SID, "SRV")).unwrap();
    assert_eq!(SaveKind::detect(&sdir), Some(SaveKind::Server));
    assert!(ServerSave::open(&sdir).unwrap().light_blocks.is_some());
    assert!(matches!(AnySave::open(&sdir).unwrap(), AnySave::Server(_)));

    // write errors: a file where a directory should be
    std::fs::write(tmp.path().join("file"), b"").unwrap();
    assert!(write_dir_files(&tmp.path().join("file"), &files).is_err());
    let mut f2 = FileMap::new();
    f2.insert("a/".into(), vec![]);
    std::fs::create_dir(tmp.path().join("d")).unwrap();
    std::fs::write(tmp.path().join("d/a"), b"").unwrap();
    assert!(write_dir_files(&tmp.path().join("d"), &f2).is_err());
    f2.clear();
    f2.insert("a/b".into(), vec![]);
    assert!(write_dir_files(&tmp.path().join("d"), &f2).is_err());
    f2.clear();
    std::fs::create_dir(tmp.path().join("d/x")).unwrap();
    f2.insert("x".into(), vec![]);
    assert!(write_dir_files(&tmp.path().join("d"), &f2).is_err());
    assert!(read_dir_files(&tmp.path().join("file")).is_err());
    assert!(save::write_atomic(&tmp.path().join("nodir/x"), b"").is_err());
    std::fs::create_dir(tmp.path().join("t")).unwrap();
    std::fs::create_dir(tmp.path().join("t/x")).unwrap();
    std::fs::write(tmp.path().join("t/x/f"), b"").unwrap();
    assert!(save::write_atomic(&tmp.path().join("t/x"), b"").is_err());
}

#[test]
fn env_helpers() {
    let mut e = client_env(CID, "A");
    assert!(get(&e, "nope", "k").is_none());
    put(&mut e, "new", b"k", b"1".to_vec());
    put(&mut e, "new", b"k", b"2".to_vec());
    assert_eq!(get(&e, "new", "k"), Some(&b"2"[..]));
    assert!(remove(&mut e, "new", b"k"));
    assert!(!remove(&mut e, "new", b"k"));
    assert!(!remove(&mut e, "nope", b"k"));
    assert_eq!(save::map_size_for(10, 64), 64);
    assert_eq!(save::map_size_for(100, 64), 256);
    assert!(env_from(&FileMap::new(), WORLD_DB).unwrap().is_none());
}

#[test]
fn map_preview() {
    let env = client_env(CID, "A");
    let (img, bad) = render_map(&env, MapOptions::default());
    assert_eq!(bad, 0);
    // cropped to blocks x 0..=2, y 0..=1
    assert_eq!((img.width, img.height), (96, 64));
    // top-left: block (0,1) is not stored
    assert_eq!(img.get(0, 0), UNKNOWN);
    // block (1,1) top rows: sky; row 20 grass; row 10 cave (x<4) in front of dirt
    let sky = tiles::TileType::AIR.color().unwrap();
    assert_eq!(img.get(40, 0), [sky[0], sky[1], sky[2], 255]);
    assert_eq!(img.get(40, 31 - 20), [80, 150, 50, 255]);
    let dirt = tiles::TileType::DIRT.color().unwrap();
    assert_eq!(img.get(33, 31 - 10), [dirt[0] / 2, dirt[1] / 2, dirt[2] / 2, 255]);
    assert_eq!(img.get(70, 5), [110, 110, 115, 255]);
    // unknown tile id at (0,0) of block (0,0) = bottom-left pixel
    assert_eq!(img.get(0, 63), UNKNOWN_TILE);

    let (img, _) = render_map(&env, MapOptions { tiles_per_pixel: 3, crop_to_content: false });
    assert_eq!((img.width, img.height), (43, 342));
    let (img, _) = render_map(&env, MapOptions { tiles_per_pixel: 0, crop_to_content: true });
    assert_eq!(img.width, 96);
    assert!(img.to_png().starts_with(b"\x89PNG"));

    // no worldv2: width from blocks; a bad block is counted
    let mut e2 = env.clone();
    remove(&mut e2, "main", b"worldv2");
    put(&mut e2, "blocks", b"5_5", b"junk".to_vec());
    let (img, bad) = render_map(&e2, MapOptions { tiles_per_pixel: 32, crop_to_content: false });
    assert_eq!((img.width, img.height, bad), (3, 32, 1));
    // no blocks at all
    let mut e3 = env.clone();
    e3.named.remove(&b"blocks"[..]);
    assert_eq!(render_map(&e3, MapOptions::default()).0.width, 0);
    // blocks outside the world are ignored
    let b = blocks::MacroBlock::empty(9, 40);
    assert_eq!(render_blocks(&[b], 4, MapOptions::default()).width, 0);
    let air = blocks::MacroBlock::empty(0, 0);
    assert_eq!(tile_color(air.tile(0, 0)), UNKNOWN_TILE);
}

#[test]
fn stored_thumbnail() {
    let env = client_env(CID, "A");
    let rec = get(&env, "main", "worldv2").unwrap();
    let mut data = vec![0u8; 320 * 320 * 4];
    data[0] = 9; // bottom-left pixel in storage order
    let mut d = plistio::parse_dict(rec).unwrap();
    d.insert("previewImageData".into(), Value::Data(data));
    let rec2 = plistio::to_xml(&Value::Dictionary(d.clone()), plistio::Dialect::Apple).unwrap();
    let img = stored_preview(&rec2).unwrap();
    assert_eq!(img.get(0, 319)[0], 9);
    d.insert("previewImageData".into(), Value::Data(vec![1, 2]));
    assert!(stored_preview(&plistio::to_xml(&Value::Dictionary(d.clone()), plistio::Dialect::Apple).unwrap()).is_none());
    d.remove("previewImageData");
    assert!(stored_preview(&plistio::to_xml(&Value::Dictionary(d), plistio::Dialect::Apple).unwrap()).is_none());
    assert!(stored_preview(b"junk").is_none());
}

#[test]
fn game_db() {
    let tmp = tempfile::tempdir().unwrap();
    let mut g = GameDb::default();
    assert!(g.world_ids().is_empty());
    let rec = client_record(CID, "HOME");
    g.register_checked(CID, &rec).unwrap();
    assert!(g.register_checked(SID, &rec).is_err());
    assert!(g.register_checked(SID, b"junk").is_err());
    put(&mut g.contents, "main", b"settings", b"keep me".to_vec());
    assert_eq!(g.world_ids(), vec![CID.to_string()]);
    assert!(g.rename(CID, "NEW").unwrap());
    assert!(!g.rename(SID, "NEW").unwrap());
    let d = plistio::parse_dict(g.entry(CID).unwrap()).unwrap();
    assert_eq!(d["worldName"].as_string(), Some("NEW"));
    let dir = tmp.path().join("game_db");
    g.save(&dir).unwrap();
    let back = GameDb::open(&dir).unwrap();
    assert_eq!(back.layout, Layout::Bits32);
    assert_eq!(back.contents.named, g.contents.named);
    // grows the map when many large records are stored
    for i in 0..40 {
        g.register(&format!("{i:032x}"), &vec![0; 500_000]);
    }
    assert!(GameDb::from_bytes(&g.to_bytes().unwrap()).unwrap().contents.map_size >= 32 << 20);
    assert!(g.unregister(CID));
    assert!(!g.unregister(CID));
    assert!(GameDb::open(&tmp.path().join("missing")).is_err());
    assert!(GameDb::from_bytes(b"junk").is_err());
    std::fs::write(tmp.path().join("f"), b"").unwrap();
    assert!(g.save(&tmp.path().join("f")).is_err());
    g.register(SID, b"junk");
    assert!(g.rename(SID, "X").is_err());
    let mut empty = GameDb::new();
    empty.contents.named.clear();
    assert!(empty.world_ids().is_empty() && empty.entry(CID).is_none());
    assert!(!empty.to_bytes().unwrap().is_empty());
}

#[test]
fn identity() {
    let new_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut c = client_files(CID, "HOME");
    set_identity(&mut c, Some(new_id), Some("COPY")).unwrap();
    let s = summarize_any(&AnySave::from_files(new_id, c.clone()).unwrap(), None);
    assert_eq!((s.id.as_str(), s.name.as_deref()), (new_id, Some("COPY")));
    assert_eq!(env(&c, WORLD_DB).1, Layout::Bits32);
    // the dialect is kept
    let rec = get(&env(&c, WORLD_DB).0, "main", "worldv2").unwrap().to_vec();
    assert_eq!(plistio::Dialect::detect(&rec), plistio::Dialect::Apple);

    let mut s = server_files(SID, "SRV");
    set_identity(&mut s, None, Some("RENAMED")).unwrap();
    let sum = summarize_any(&AnySave::from_files(SID, s.clone()).unwrap(), None);
    assert_eq!(sum.name.as_deref(), Some("RENAMED"));
    assert_eq!(plistio::parse_dict(&s["worldv2"]).unwrap()["worldName"].as_string(), Some("RENAMED"));
    let rec = get(&env(&s, WORLD_DB).0, "main", "worldv2").unwrap().to_vec();
    assert_eq!(plistio::Dialect::detect(&rec), plistio::Dialect::GnuStep);

    assert!(set_identity(&mut c, Some("bad"), None).is_err());
    assert!(set_identity(&mut FileMap::new(), None, Some("x")).is_err());
    let mut e = client_env(CID, "A");
    remove(&mut e, "main", b"worldv2");
    let mut f = FileMap::new();
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(set_identity(&mut f, None, Some("x")).unwrap_err().to_string().contains("no worldv2"));
    // a server world never run: only the file changes
    let mut e = server_env(SID, "A");
    remove(&mut e, "main", b"worldv2");
    let mut f = server_files(SID, "A");
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits64).unwrap());
    let before = f[WORLD_DB].clone();
    set_identity(&mut f, None, Some("B")).unwrap();
    assert_eq!(f[WORLD_DB], before);
    f.insert("worldv2".into(), b"junk".to_vec());
    assert!(set_identity(&mut f, None, Some("B")).is_err());
    let mut c = client_files(CID, "HOME");
    let mut e = client_env(CID, "A");
    put(&mut e, "main", b"worldv2", b"junk".to_vec());
    c.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(set_identity(&mut c, None, Some("B")).is_err());
}

#[test]
fn record_rewrite_keeps_the_other_bytes() {
    use bhw_save::identity::rewrite_record;
    let new_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let rec = client_record(CID, "HOME");
    let out = rewrite_record(&rec, Some(new_id), Some("A & <B>")).unwrap();
    let back = String::from_utf8(out.clone()).unwrap().replace(new_id, CID).replace("A &amp; &lt;B&gt;", "HOME");
    assert_eq!(back.into_bytes(), rec, "only the two values change");
    let d = plistio::parse_dict(&out).unwrap();
    assert_eq!((d["saveID"].as_string(), d["worldName"].as_string()), (Some(new_id), Some("A & <B>")));
    assert_eq!(rewrite_record(&rec, None, None).unwrap(), rec);
    // A missing key or a non-string value: parsed and written again.
    let mut r = record(CID, "HOME");
    r.remove("worldName");
    r.insert("saveID".into(), plist::Value::Integer(5.into()));
    let odd = plistio::to_xml(&plist::Value::Dictionary(r), plistio::Dialect::Apple).unwrap();
    let out = rewrite_record(&odd, Some(new_id), Some("N")).unwrap();
    let d = plistio::parse_dict(&out).unwrap();
    assert_eq!((d["saveID"].as_string(), d["worldName"].as_string()), (Some(new_id), Some("N")));
    // The first `worldName` is in a nested dict: the textual edit is checked and dropped.
    let mut nested = plist::Dictionary::new();
    nested.insert("worldName".into(), plist::Value::String("inner".into()));
    let mut r = plist::Dictionary::new();
    r.insert("aNested".into(), plist::Value::Dictionary(nested));
    r.extend(record(CID, "HOME"));
    let odd = plistio::to_xml(&plist::Value::Dictionary(r), plistio::Dialect::Apple).unwrap();
    let d = plistio::parse_dict(&rewrite_record(&odd, None, Some("N")).unwrap()).unwrap();
    assert_eq!(d["worldName"].as_string(), Some("N"));
    // An unterminated string is not touched textually (and the record does not parse).
    assert!(rewrite_record(b"<key>saveID</key><string>x", Some(new_id), None).is_err());
}

#[test]
fn round_trip_client_server_client() {
    let client = client_files(CID, "HOME");
    let (server, rep) = client_to_server_files(&client, &opts()).unwrap();
    assert!(!rep.has_losses());
    assert!(rep.notes.iter().any(|n| n.kind == NoteKind::SetAside));
    assert!(rep.notes.iter().any(|n| n.message.contains("previewImageData")));
    let (w, layout) = env(&server, WORLD_DB);
    assert_eq!(layout, Layout::Bits64);
    assert_eq!(w.map_size, 1 << 30);
    assert!(same(&w, &client_env(CID, "HOME")));
    assert_eq!(env(&server, SERVER_DB).0.named[&b"main"[..]].entries.len(), 0);
    assert_eq!(env(&server, LIGHT_BLOCKS).0.map_size, 1 << 30);
    let file = plistio::parse_dict(&server["worldv2"]).unwrap();
    assert_eq!(file["saveID"].as_string(), Some(CID));
    assert!(!file.contains_key("randomSeed"));
    assert!(server.contains_key(&format!("{CLIENT_EXTRAS}/map/default/33_33.png")));
    assert!(!server.keys().any(|k| k.ends_with("lock.mdb")));
    assert_eq!(summarize_any(&AnySave::from_files(CID, server.clone()).unwrap(), None).kind, Some(SaveKind::Server));

    let (back, rep) = server_to_client_files(&server, &opts()).unwrap();
    assert!(rep.notes.iter().any(|n| n.message.contains("restored the client files")));
    let (w, layout) = env(&back, WORLD_DB);
    assert_eq!(layout, Layout::Bits32);
    assert_eq!(w.map_size, 64 << 20);
    assert!(same(&w, &client_env(CID, "HOME")));
    assert_eq!(back["map/default/33_33.png"], client["map/default/33_33.png"]);
    // the server-only files made from the client world are set aside
    assert!(back.contains_key(&format!("{SERVER_EXTRAS}/worldv2")));
    // ...and restored on the next trip
    let (server2, rep) = client_to_server_files(&back, &opts()).unwrap();
    assert!(rep.notes.iter().any(|n| n.message.contains("restored the server files")));
    assert_eq!(server2["worldv2"], server["worldv2"]);
    assert_eq!(server2[SERVER_DB], server[SERVER_DB]);

    // renamed on the client side: the restored worldv2 file follows
    let mut renamed = back.clone();
    set_identity(&mut renamed, None, Some("OTHER")).unwrap();
    let (server3, _) = client_to_server_files(&renamed, &opts()).unwrap();
    assert_eq!(plistio::parse_dict(&server3["worldv2"]).unwrap()["worldName"].as_string(), Some("OTHER"));
    let mut broken = back.clone();
    broken.insert(format!("{SERVER_EXTRAS}/worldv2"), b"junk".to_vec());
    assert!(client_to_server_files(&broken, &opts()).is_err());

    // without extras: reported as lost
    let (server4, rep) =
        client_to_server_files(&client, &ConvertOptions { keep_extras: false, adopt_player: None }).unwrap();
    assert!(rep.has_losses());
    assert!(!server4.keys().any(|k| k.starts_with(CLIENT_EXTRAS)));
}

#[test]
fn round_trip_server_client_server() {
    let server = server_files(SID, "SRV");
    let (client, rep) = server_to_client_files(&server, &opts()).unwrap();
    assert!(!rep.has_losses());
    assert!(rep.notes.iter().any(|n| n.message.contains("1 player record")));
    assert!(rep.notes.iter().any(|n| n.message.contains("per-player")));
    assert!(rep.notes.iter().any(|n| n.message.contains("thumbnail")));
    assert!(client.contains_key("map/default/"));
    let s = summarize_any(&AnySave::from_files(SID, client.clone()).unwrap(), None);
    assert_eq!(s.kind, Some(SaveKind::Client));
    let (again, _) = client_to_server_files(&client, &opts()).unwrap();
    for k in [SERVER_DB, LIGHT_BLOCKS, "worldv2", "adminlist.txt"] {
        assert_eq!(again[k], server[k], "{k}");
    }
    assert!(same(&env(&again, WORLD_DB).0, &server_env(SID, "SRV")));
    // the empty map directory came back as client extras
    assert!(!again.keys().any(|k| k.starts_with(CLIENT_EXTRAS)));

    let (_, rep) = server_to_client_files(&server, &ConvertOptions { keep_extras: false, adopt_player: None }).unwrap();
    assert!(rep.has_losses());

    // a client cache of a remote world fed back is still refused; empty server_db reports no players
    let mut s2 = server.clone();
    let mut sdb = env(&s2, SERVER_DB).0;
    sdb.named.clear();
    s2.insert(SERVER_DB.into(), bhw_lmdb::write_env(&sdb, Layout::Bits64).unwrap());
    let (_, rep) = server_to_client_files(&s2, &opts()).unwrap();
    assert!(!rep.notes.iter().any(|n| n.message.contains("in server_db")), "{rep:?}");
}

#[test]
fn adopt_player() {
    let server = server_files(SID, "SRV");
    let o = ConvertOptions { keep_extras: true, adopt_player: Some("p1".into()) };
    let (client, rep) = server_to_client_files(&server, &o).unwrap();
    assert!(rep.has_losses(), "NOINV has no inventory");
    let w = env(&client, WORLD_DB).0;
    let names: Vec<String> =
        summary::blockheads_in(get(&w, "main", "blockheads").unwrap()).unwrap().into_iter().map(|b| b.0).collect();
    assert_eq!(names, ["SCOTT", "NOINV"]);
    assert_eq!(get(&w, "main", "blockhead_7_inventory").unwrap(), inventory());
    let o = ConvertOptions { keep_extras: true, adopt_player: Some("nobody".into()) };
    assert!(server_to_client_files(&server, &o).is_err());
    // all inventories present: no loss
    let mut e = server_env(SID, "SRV");
    put(&mut e, "main", b"p1_blockheads", plistio::gzip(&blockheads(&[("SCOTT", 7)])));
    let mut s2 = server.clone();
    s2.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits64).unwrap());
    let o = ConvertOptions { keep_extras: true, adopt_player: Some("p1".into()) };
    assert!(!server_to_client_files(&s2, &o).unwrap().1.has_losses());
    // corrupt inventory / record
    put(&mut e, "main", b"p1_blockhead_7_inventory", vec![0x1f, 0x8b, 1]);
    s2.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits64).unwrap());
    assert!(server_to_client_files(&s2, &o).is_err());
    put(&mut e, "main", b"p1_blockheads", vec![0x1f, 0x8b, 1]);
    s2.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits64).unwrap());
    assert!(server_to_client_files(&s2, &o).is_err());
    put(&mut e, "main", b"p1_blockheads", b"junk".to_vec());
    s2.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits64).unwrap());
    assert!(server_to_client_files(&s2, &o).is_err());
}

#[test]
fn conversion_errors() {
    let mut e = client_env(CID, "A");
    remove(&mut e, "main", b"worldv2");
    let mut f = FileMap::new();
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(matches!(client_to_server_files(&f, &opts()), Err(Error::Unsupported(_))));
    assert!(client_to_server_files(&FileMap::new(), &opts()).is_err());
    assert!(server_to_client_files(&FileMap::new(), &opts()).is_err());
    // a broken blockheads record stops the conversion (it would be parsed for the report)
    let mut e = client_env(CID, "A");
    put(&mut e, "main", b"blockheads", b"junk".to_vec());
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(client_to_server_files(&f, &opts()).is_err());
    // no single-player blockheads and no client-only keys: fewer notes
    let mut e = client_env(CID, "A");
    remove(&mut e, "main", b"blockheads");
    put(
        &mut e,
        "main",
        b"worldv2",
        plistio::to_xml(&Value::Dictionary(record(CID, "A")), plistio::Dialect::Apple).unwrap(),
    );
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(client_to_server_files(&f, &opts()).unwrap().1.notes.is_empty());
}

#[test]
fn folder_conversion() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("client");
    write_dir_files(&src, &client_files(CID, "HOME")).unwrap();
    let dst = tmp.path().join("server");
    std::fs::create_dir(tmp.path().join("server.bhw-tmp")).unwrap(); // leftover from an interrupted run
    client_to_server(&src, &dst, &opts()).unwrap();
    assert!(dst.join("server_db/data.mdb").is_file());
    assert!(matches!(client_to_server(&src, &dst, &opts()), Err(Error::Io { .. })));
    let back = tmp.path().join("back");
    server_to_client(&dst, &back, &opts()).unwrap();
    assert!(back.join("map/default/33_33.png").is_file());
    assert!(client_to_server(&tmp.path().join("missing"), &tmp.path().join("x"), &opts()).is_err());
    assert!(server_to_client(&tmp.path().join("missing"), &tmp.path().join("x"), &opts()).is_err());
    // the temporary sibling cannot be removed: a file in its place is not a directory
    std::fs::write(tmp.path().join("y.bhw-tmp"), b"").unwrap();
    assert!(client_to_server(&src, &tmp.path().join("y"), &opts()).is_err());
    // rename fails: target appears as a non-empty directory after the check is impossible to stage; instead use a
    // destination below a file
    assert!(client_to_server(&src, &tmp.path().join("y.bhw-tmp/z"), &opts()).is_err());
}

#[test]
fn world_traits() {
    use bhw_world::meta::ServerWorldConverter;
    use bhw_world::{WorldMetaEditor, WorldMetaSource};
    let files: Vec<bhw_world::WorldFile> = client_files(CID, "HOME")
        .into_iter()
        .map(|(path, data)| bhw_world::WorldFile { path, data: bhw_world::Payload::Bytes(data) })
        .collect();
    let mut wf = bhw_world::WorldFiles::new(CID.into(), files).unwrap();
    let m = world::SaveMeta::default();
    let meta = m.read_meta(&wf).unwrap();
    assert_eq!(meta.name.as_deref(), Some("HOME"));
    assert_eq!(meta.created_at.as_deref(), Some("2026-10-20T08:48:09Z"));
    assert_eq!(meta.game_version, None);
    m.set_identity(&mut wf, Some(SID), Some("COPY")).unwrap();
    assert_eq!(m.read_meta(&wf).unwrap().name.as_deref(), Some("COPY"));
    let server = m.game_to_server(&wf).unwrap();
    assert!(server.is_server_world());
    let game = m.server_to_game(&server).unwrap();
    assert!(!game.is_server_world() && game.is_world());
    assert!(game.get("map/default/").is_none());
    assert!(m.set_identity(&mut wf, Some("bad"), None).is_err());
    let empty = bhw_world::WorldFiles::new(CID.into(), vec![]).unwrap();
    assert!(m.read_meta(&empty).is_err());
    assert!(m.game_to_server(&empty).is_err());
    assert!(m.server_to_game(&empty).is_err());
    let missing = bhw_world::WorldFiles::new(
        CID.into(),
        vec![bhw_world::WorldFile { path: "x".into(), data: bhw_world::Payload::File("/nonexistent/x".into()) }],
    )
    .unwrap();
    assert!(world::to_file_map(&missing).is_err());
}

#[test]
fn sparse_worlds() {
    // server world without server_db; client world without blocks or main
    let mut s = server_files(SID, "SRV");
    s.remove(SERVER_DB);
    let (_, rep) = server_to_client_files(&s, &opts()).unwrap();
    assert!(!rep.notes.iter().any(|n| n.message.contains("in server_db")));
    let save = AnySave::from_files(SID, s).unwrap();
    assert_eq!(save.world_db().named.len(), 3);
    let mut e = bhw_lmdb::EnvContents::new(4096, 64 << 20);
    e.named.insert(b"dw".to_vec(), bhw_lmdb::Database::new(0));
    let mut f = FileMap::new();
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    let sum = summarize_any(&AnySave::from_files(CID, f).unwrap(), None);
    assert_eq!((sum.block_count, sum.blockheads.len(), sum.name), (0, 0, None));
}

#[test]
fn fitting_and_register_save() {
    assert_eq!(MapOptions::fitting(16384, 1000).tiles_per_pixel, 17);
    assert_eq!(MapOptions::fitting(10, 0).tiles_per_pixel, 10);
    assert_eq!(MapOptions::fitting(10, 1000).tiles_per_pixel, 1);
    let mut g = GameDb::new();
    let save = ClientSave::from_files(CID, client_files(CID, "HOME")).unwrap();
    assert_eq!(g.register_save(&save).unwrap(), CID);
    assert_eq!(g.entry(CID), save.worldv2());
    let mut e = client_env(CID, "A");
    let mut r = record(CID, "A");
    r.remove("saveID");
    put(&mut e, "main", b"worldv2", plistio::to_xml(&Value::Dictionary(r), plistio::Dialect::Apple).unwrap());
    let mut f = FileMap::new();
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(g.register_save(&ClientSave::from_files(CID, f.clone()).unwrap()).is_err());
    put(&mut e, "main", b"worldv2", b"junk".to_vec());
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(g.register_save(&ClientSave::from_files(CID, f.clone()).unwrap()).is_err());
    remove(&mut e, "main", b"worldv2");
    f.insert(WORLD_DB.into(), bhw_lmdb::write_env(&e, Layout::Bits32).unwrap());
    assert!(g.register_save(&ClientSave::from_files(CID, f).unwrap()).is_err());
}
