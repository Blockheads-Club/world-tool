//! The app's flows against a real Blockheads PC data folder. Skipped unless `BHW_PC_DIR` names one (the folder that
//! holds `home/`, e.g. `~/.local/share/blockheads-pc`):
//!
//! ```sh
//! BHW_PC_DIR=~/.local/share/blockheads-pc cargo test -p bhw-core --test real_pc -- --nocapture
//! ```
//!
//! The folder is **never written**: its `home/` is copied into a temporary folder first and every flow runs on the
//! copy (read, save to the library, export `.bhworld`, copy a world in as "keep both", edit it in the editor and copy
//! the edit over the copy, restore the automatic backup); the real folder's files are checksummed before and after.
//! `BHW_PC_KEEP=<dir>` keeps the copy with the edited world in `<dir>` (must not exist) to open it in the game
//! (`bhpc-run --config launcher.toml` with `data_dir` set to `<dir>`); the restore is skipped then.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bhw_core::editor::EditRequest;
use bhw_core::{
    Choice, Core, CoreConfig, DeviceKind, ExportFormat, ExportRequest, ReadRequest, Source, TransferRequest,
};
use bhw_device::process::SystemRunner;
use bhw_device::Adb;
use bhw_editor::{Brush, OpKind};
use bhw_save::GameDb;

/// Every file below `dir` with its contents (relative path → bytes).
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let ty = e.file_type().unwrap();
            if ty.is_dir() {
                walk(root, &e.path(), out);
            } else if ty.is_file() {
                out.insert(e.path().strip_prefix(root).unwrap().to_path_buf(), std::fs::read(e.path()).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let ty = e.file_type().unwrap();
        if ty.is_dir() {
            copy_tree(&e.path(), &dst.join(e.file_name()));
        } else if ty.is_file() {
            std::fs::copy(e.path(), dst.join(e.file_name())).unwrap();
        }
    }
}

fn core_in(dir: &Path, pc: PathBuf) -> Core {
    let core = Core::new(CoreConfig {
        config_dir: dir.join("config"),
        cache_dir: dir.join("cache"),
        data_dir: dir.join("data"),
        default_worlds_folder: dir.join("Worlds"),
        adb: None,
        make_adb: Arc::new(|p| Adb::new(p, Arc::new(SystemRunner))),
        fetch: Arc::new(bhw_core::adb_tools::NoFetch),
        runner: Arc::new(SystemRunner),
        pc_locator: Arc::new(move || Some(pc.clone())),
        ios_locator: Arc::new(Vec::new),
        android_dir: None,
        app_version: "test".into(),
    });
    let patch = serde_json::json!({ "editor_generate": true });
    core.set_settings(patch.as_object().unwrap()).unwrap();
    core
}

/// World names on the PC copy, sorted.
fn names(core: &Core) -> (String, Vec<String>) {
    let req = ReadRequest { device_id: "pc".into(), password: String::new() };
    let r = core.read("read", &req, &mut |_| {}).expect("read");
    let mut n: Vec<String> = r.worlds.iter().map(|w| w.name.clone()).collect();
    n.sort();
    (r.read_id, n)
}

/// `world_db/data.mdb` of every world plus the world list, the state a restore must bring back.
fn saves_state(data: &Path) -> (BTreeMap<String, Vec<u8>>, Vec<String>) {
    let support = data.join("home/Library/Application Support");
    let mut dbs = BTreeMap::new();
    for e in std::fs::read_dir(support.join("saves")).unwrap() {
        let e = e.unwrap();
        let db = e.path().join("world_db/data.mdb");
        if db.is_file() {
            dbs.insert(e.file_name().to_string_lossy().into_owned(), std::fs::read(db).unwrap());
        }
    }
    let mut ids = GameDb::open(&support.join("game_db")).unwrap().world_ids();
    ids.sort();
    (dbs, ids)
}

#[test]
fn flows_on_a_copy_of_blockheads_pc() {
    let Some(real) = std::env::var_os("BHW_PC_DIR").filter(|s| !s.is_empty()).map(PathBuf::from) else {
        eprintln!("BHW_PC_DIR not set; skipping");
        return;
    };
    assert!(real.join("home").is_dir(), "{} has no home/ folder", real.display());
    let real_before = snapshot(&real.join("home"));
    let keep = std::env::var_os("BHW_PC_KEEP").map(PathBuf::from);
    let tmp = tempfile::tempdir().unwrap();
    let data = keep.clone().unwrap_or_else(|| tmp.path().join("pc"));
    assert!(!data.exists(), "{} exists", data.display());
    copy_tree(&real.join("home"), &data.join("home"));
    let core = core_in(tmp.path(), data.clone());

    // The PC install is listed with its worlds.
    let pc = core.list_devices().into_iter().find(|d| d.kind == DeviceKind::Pc).expect("Blockheads PC listed");
    eprintln!("device: {pc:?}");
    let (read_id, before) = names(&core);
    eprintln!("worlds: {before:?}");
    assert!(!before.is_empty());
    assert_eq!(pc.world_count, Some(before.len()));
    let state_before = saves_state(&data);

    // Library + .bhworld export of every world.
    let ids: Vec<String> =
        core.source_worlds(&Source::Device { device_id: "pc".into() }).unwrap().into_iter().map(|w| w.id).collect();
    let saved = core.save_to_library(&read_id, &ids).unwrap();
    assert_eq!(saved.len(), before.len());
    let req = ExportRequest {
        read_id: None,
        world_ids: saved.iter().map(|w| w.base.id.clone()).collect(),
        format: ExportFormat::Bhworld,
        folder: tmp.path().join("export").display().to_string(),
    };
    let r = core.export_worlds("export", &req, &mut |_| {}).unwrap();
    eprintln!("exported: {:?}", r.files);
    assert_eq!(r.files.len(), saved.len());

    // An exported file goes back in as a second copy ("keep both").
    let file = tmp.path().join("export").join(&r.files[0].name).display().to_string();
    let src = Source::Files { paths: vec![file.clone()] };
    let plan = core.transfer_plan(&src, std::slice::from_ref(&file), "pc").unwrap();
    let copy_name = plan.items[0].copy_name.clone().expect("a copy name, the world is there already");
    let req = TransferRequest { plan_id: plan.plan_id, choices: Default::default() };
    let res = core.transfer("copy", &req, &mut |p| eprintln!("copy: {p:?}")).expect("copy");
    eprintln!("copy: {res:?}");
    assert_eq!(res.kept_both, vec![copy_name.clone()]);
    let backup_id = res.backup_id.clone().expect("automatic backup");
    let (_, after) = names(&core);
    assert!(after.contains(&copy_name), "{after:?}");
    assert_eq!(after.len(), before.len() + 1);

    // The copy is edited in the editor (a stone platform above its spawn) and copied over itself.
    let (read_id, _) = names(&core);
    let copy = core
        .source_worlds(&Source::Device { device_id: "pc".into() })
        .unwrap()
        .into_iter()
        .find(|w| w.name == copy_name)
        .unwrap();
    let lib = core.save_to_library(&read_id, std::slice::from_ref(&copy.id)).unwrap();
    let ew = core.editor_open(&lib[0].base.id).unwrap();
    let [sx, sy] = ew.spawn.expect("spawn");
    let (sx, sy) = (sx as u32, sy as u32);
    let pts: Vec<[u32; 2]> = (0..9).map(|i| [sx - 4 + i, sy + 6]).collect();
    let o = core
        .editor_edit(
            &ew.session,
            EditRequest::Paint { kind: OpKind::Pencil, points: pts, brush: Brush::front(10), only_solid: false },
        )
        .unwrap()
        .outcome;
    eprintln!("edit at {sx},{}: {o:?}", sy + 6);
    let edited = core.editor_save(&ew.session, false).unwrap();
    let plan = core.transfer_plan(&Source::Library, std::slice::from_ref(&edited.base.id), "pc").unwrap();
    let key = plan.items[0].key.clone();
    let req = TransferRequest { plan_id: plan.plan_id, choices: [(key, Choice::Replace)].into_iter().collect() };
    let res = core.transfer("edit", &req, &mut |_| {}).expect("copy of the edit");
    eprintln!("edit copied: {res:?}");
    assert_eq!(res.replaced, vec![copy_name.clone()]);
    let (read_id, again) = names(&core);
    assert_eq!(again, after);
    let ew = core.editor_open(&core.save_to_library(&read_id, std::slice::from_ref(&copy.id)).unwrap()[0].base.id);
    let ew = ew.unwrap();
    let tile = core.editor_tile(&ew.session, sx, sy + 6).unwrap();
    eprintln!("tile after the copy: {tile:?}");
    assert_eq!(tile.fg, 10);
    assert!(state_before.0.keys().all(|id| data.join("home/Library/Application Support/saves").join(id).is_dir()));

    if let Some(keep) = &keep {
        eprintln!(
            "kept the copy with \"{copy_name}\" (stone at x {}..{}, y {}) in {}",
            sx - 4,
            sx + 4,
            sy + 6,
            keep.display()
        );
    } else {
        // The automatic backup puts everything back.
        core.restore_backup("restore", &backup_id, "pc", &mut |_| {}).expect("restore");
        let (_, restored) = names(&core);
        assert_eq!(restored, before);
        let (dbs, ids) = saves_state(&data);
        assert_eq!(ids, state_before.1, "world list after the restore");
        let keys = |m: &BTreeMap<String, Vec<u8>>| m.keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&dbs), keys(&state_before.0), "world folders after the restore");
        // The files are rewritten (repacked, both meta pages current), their contents must be the same.
        let contents = |b: &[u8]| bhw_lmdb::Env::open(b).unwrap().read_all().unwrap();
        let changed: Vec<&String> =
            dbs.iter().filter(|(k, v)| contents(&state_before.0[*k]) != contents(v)).map(|(k, _)| k).collect();
        assert!(changed.is_empty(), "world_db contents changed by the restore: {changed:?}");
    }
    let real_after = snapshot(&real.join("home"));
    let changed: Vec<_> = real_after.iter().filter(|(k, v)| real_before.get(*k) != Some(v)).map(|(k, _)| k).collect();
    assert!(changed.is_empty() && real_after.len() == real_before.len(), "the real folder changed: {changed:?}");
}
