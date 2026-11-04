//! The editor's whole path on a real phone or emulator: read a world through root, edit it in the editor (blocks,
//! an inventory, a tree removed with its tile, terrain in an area the game never generated), save it to the
//! library and copy it back over the original. Skipped unless `BHW_EDITOR_DEVICE` names a rooted device
//! (`adb root`, e.g. the test emulator):
//!
//! ```sh
//! BHW_EDITOR_DEVICE=emulator-5600 cargo test -p bhw-core --test editor_device -- --nocapture
//! ```
//!
//! **Changes the device's game data.** Afterwards open the world in the game and check it loads (the test prints
//! the positions to look at).

use std::sync::Arc;

use bhw_core::editor::{EditRequest, SearchKind};
use bhw_core::{Choice, Core, CoreConfig, ReadRequest, Source, TransferRequest};
use bhw_device::process::SystemRunner;
use bhw_device::Adb;
use bhw_editor::{Brush, OpKind};

fn core_in(dir: &std::path::Path) -> Core {
    let core = Core::new(CoreConfig {
        config_dir: dir.join("config"),
        cache_dir: dir.join("cache"),
        data_dir: dir.join("data"),
        default_worlds_folder: dir.join("Worlds"),
        adb: Some(Adb::locate(None).expect("adb")),
        make_adb: Arc::new(|p| Adb::new(p, Arc::new(SystemRunner))),
        fetch: Arc::new(bhw_core::adb_tools::NoFetch),
        runner: Arc::new(SystemRunner),
        pc_locator: Arc::new(|| None),
        ios_locator: Arc::new(Vec::new),
        android_dir: None,
        app_version: "test".into(),
    });
    let patch = serde_json::json!({ "root_mode": "auto", "editor_generate": true, "editor_objects": true });
    core.set_settings(patch.as_object().unwrap()).unwrap();
    core
}

#[test]
fn edit_and_copy_back() {
    let Some(serial) = std::env::var("BHW_EDITOR_DEVICE").ok().filter(|s| !s.is_empty()) else {
        eprintln!("BHW_EDITOR_DEVICE not set; skipping");
        return;
    };
    let world_name = std::env::var("BHW_EDITOR_WORLD").unwrap_or_else(|_| "MY FIRST WORLD".into());
    let tmp = tempfile::tempdir().unwrap();
    let core = core_in(tmp.path());
    let r =
        core.read("read", &ReadRequest { device_id: serial.clone(), password: String::new() }, &mut |_| {}).unwrap();
    let w = r.worlds.iter().find(|w| w.name == world_name).expect("world on the device");
    let saved = core.save_to_library(&r.read_id, std::slice::from_ref(&w.id)).unwrap();
    let lib_id = saved[0].base.id.clone();

    let ew = core.editor_open(&lib_id).unwrap();
    let s = ew.session.clone();
    let [sx, sy] = ew.spawn.expect("spawn");
    let (sx, sy) = (sx as u32, sy as u32);
    let edit = |req: EditRequest| core.editor_edit(&s, req).unwrap().outcome;
    // 1. a red brick arch above the spawn
    let pts: Vec<[u32; 2]> = (0..7).map(|i| [sx - 3 + i, sy + 6]).chain([[sx - 3, sy + 5], [sx + 3, sy + 5]]).collect();
    let o = edit(EditRequest::Paint { kind: OpKind::Pencil, points: pts, brush: Brush::front(11), only_solid: false });
    eprintln!("bricks: {o:?}");
    // 2. inventory: 5 apples in the first blockhead's slot 4
    let bh = &ew.blockheads[0];
    edit(EditRequest::SetStack { player: bh.player.clone(), id: bh.id, slot: 4, item: 21, count: 5 });
    // 3. the tree nearest to the spawn is replaced by its base tile turning into rock
    let trees: Vec<_> = core
        .editor_search(&s, SearchKind::Objects, "tree")
        .unwrap()
        .into_iter()
        .filter(|o| o.kind_name.as_deref().is_some_and(|n| n.ends_with("_tree")))
        .collect();
    let tree = trees.iter().min_by_key(|o| (o.x - sx as i64).abs()).expect("a tree");
    let o = edit(EditRequest::Paint {
        kind: OpKind::Pencil,
        points: vec![[tree.x as u32, tree.y as u32]],
        brush: Brush::front(1),
        only_solid: false,
    });
    eprintln!("tree {:?} at {},{}: {o:?}", tree.id, tree.x, tree.y);
    assert_eq!(o.removed, 1);
    // 4. a stone platform in the nearest macro column the game has not generated
    let mx = (sx / 32 + 1..ew.width / 32).find(|mx| !ew.blocks.contains(&[*mx, sy / 32])).expect("ungenerated column");
    let pts: Vec<[u32; 2]> = (0..32).flat_map(|dx| (sy - 8..sy).map(move |y| [mx * 32 + dx, y])).collect();
    let o = edit(EditRequest::Paint { kind: OpKind::Pencil, points: pts, brush: Brush::front(10), only_solid: false });
    eprintln!("ungenerated column {mx}: {o:?}");
    assert!(o.created > 0);
    core.editor_save(&s, false).unwrap();

    // copy it back over the original
    let plan = core.transfer_plan(&Source::Library, std::slice::from_ref(&lib_id), &serial).unwrap();
    let key = plan.items[0].key.clone();
    let req = TransferRequest { plan_id: plan.plan_id, choices: [(key, Choice::Replace)].into_iter().collect() };
    let res = core.transfer("copy", &req, &mut |p| eprintln!("copy: {p:?}")).unwrap();
    eprintln!("copied: {res:?}");
    eprintln!(
        "look at: bricks above {sx},{sy}; tree gone at {},{}; stone platform at x {}..{} y {}..{}",
        tree.x,
        tree.y,
        mx * 32,
        mx * 32 + 31,
        sy - 8,
        sy - 1
    );
}
