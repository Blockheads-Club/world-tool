use super::*;
use crate::brush::Layer;
use crate::session::EditOptions;
use bhw_content::fixtures::world_env;
use plist::Value;

fn session() -> Session {
    Session::new(WorldContent::load(world_env()).unwrap())
}

const Y: u32 = 512; // bottom row of the stored blocks (macro y 16); x 0..32 rock, 32..64 air

#[test]
fn paint_undo_redo() {
    let mut s = session();
    assert_eq!((s.width(), s.height()), (128, 1024));
    assert_eq!(s.tile(500, 0).unwrap(), None);
    assert_eq!(s.tile(5, 2000).unwrap(), None);
    let o = s
        .paint(OpKind::Pencil, &[(1, Y), (1, Y), (2, Y), (40, Y), (100, Y), (500, Y)], Brush::front(6), false)
        .unwrap();
    assert_eq!(o, Outcome { changed: 3, protected: 0, missing: 2, ..Default::default() });
    assert_eq!(s.tile(1, Y).unwrap().unwrap().fg(), 6);
    assert_eq!(s.unsaved(), 1);
    // only solid: air at 40 is skipped
    let o = s.paint(OpKind::Brush, &[(41, Y), (3, Y)], Brush::front(7), true).unwrap();
    assert_eq!(o.changed, 1);
    // painting the same again changes nothing and adds no step
    let o = s.paint(OpKind::Brush, &[(3, Y)], Brush::front(7), true).unwrap();
    assert_eq!(o.changed, 0);
    let (labels, cur) = s.history();
    assert_eq!((labels.len(), cur), (2, 2));
    assert_eq!(labels[0].a.as_deref(), Some("t:6"));
    assert_eq!(labels[0].tiles, 3);
    assert_eq!(s.changed_tiles(), vec![(1, Y), (2, Y), (3, Y), (40, Y)]);
    assert_eq!(s.take_touched(), vec![(0, 16), (1, 16)]);
    assert!(s.take_touched().is_empty());
    assert_eq!(s.undo().unwrap().unwrap().kind, OpKind::Brush);
    assert_eq!(s.tile(3, Y).unwrap().unwrap().fg(), 1);
    assert_eq!(s.redo().unwrap().unwrap().kind, OpKind::Brush);
    assert_eq!(s.redo().unwrap(), None);
    s.go_to(0).unwrap();
    assert_eq!(s.tile(1, Y).unwrap().unwrap().fg(), 1);
    assert!(s.changed_tiles().is_empty());
    assert_eq!(s.undo().unwrap(), None);
    s.go_to(2).unwrap();
    assert_eq!(s.tile(3, Y).unwrap().unwrap().fg(), 7);
    assert!(s.go_to(3).is_err());
    // eraser and invalid brushes
    let o = s.paint(OpKind::Remove, &[(5, Y)], Brush::eraser(), false).unwrap();
    assert_eq!(o.changed, 1);
    assert_eq!(s.history().0[2].a, None);
    assert!(s.paint(OpKind::Pencil, &[(5, Y)], Brush::front(0), false).is_err());
    // saving clears the unsaved count and writes the change
    let env = s.save().unwrap();
    assert_eq!(s.unsaved(), 0);
    let mut back = WorldContent::load(env).unwrap();
    assert_eq!(back.blocks.tile(3, Y).unwrap().unwrap().fg(), 7);
}

#[test]
fn areas() {
    let mut s = session();
    let r = Rect { x: 30, y: Y, w: 4, h: 2 };
    assert_eq!(s.fill(r, Brush::front(6)).unwrap().changed, 8);
    let back = Brush { layer: Layer::Back, tile: 6, contents: 0 };
    assert_eq!(s.fill(r, back).unwrap().changed, 8);
    assert_eq!(s.history().0[1].a.as_deref(), Some("b:6"));
    assert_eq!(s.clear(r).unwrap().changed, 8);
    assert!(s.fill(Rect { x: 0, y: 0, w: 0, h: 1 }, Brush::front(6)).is_err());
    assert!(s.fill(Rect { x: 0, y: 0, w: 4096, h: 1024 }, Brush::front(6)).is_err());
    assert!(s.fill(r, Brush::front(0)).is_err());
    assert!(s.clear(Rect { x: 0, y: 0, w: 0, h: 0 }).is_err());
    // a fill over an ungenerated area counts the missing tiles
    let o = s.fill(Rect { x: 60, y: Y, w: 8, h: 1 }, Brush::front(6)).unwrap();
    assert_eq!((o.changed, o.missing), (4, 4));
}

#[test]
fn replace_and_find() {
    let mut s = session();
    let rock = Match { layer: Layer::Front, tile: 1, contents: None };
    let (found, n) = s.find(rock, None, 3).unwrap();
    assert_eq!((found.len(), n), (3, 1024));
    let (_, n) = s.find(rock, Some(Rect { x: 0, y: Y, w: 64, h: 1 }), 10).unwrap();
    assert_eq!(n, 32);
    let air = Match { layer: Layer::Front, tile: 2, contents: None };
    let (_, n) = s.find(air, Some(Rect { x: 60, y: Y, w: 8, h: 1 }), 10).unwrap();
    assert_eq!(n, 4, "ungenerated tiles never match");
    assert!(s.find(rock, Some(Rect { x: 0, y: 0, w: 0, h: 0 }), 1).is_err());
    let o = s
        .replace(rock, Brush { layer: Layer::Front, tile: 1, contents: 61 }, Some(Rect { x: 0, y: Y, w: 2, h: 1 }))
        .unwrap();
    assert_eq!(o.changed, 2);
    let l = &s.history().0[0];
    assert_eq!((l.a.as_deref(), l.b.as_deref()), (Some("t:1"), Some("t:1:61")));
    let o = s.replace(rock, Brush::front(6), None).unwrap();
    assert_eq!((o.changed, o.protected), (1023, 1), "the chest's tile is protected");
    assert!(s.replace(rock, Brush::front(0), None).is_err());
}

#[test]
fn protected_and_limits() {
    let mut s = session();
    let mut t = s.tile(10, Y).unwrap().unwrap();
    t.set_contents(46); // workbench
    s.world.blocks.set_tile(10, Y, t).unwrap();
    let o = s.paint(OpKind::Pencil, &[(10, Y), (11, Y)], Brush::front(6), false).unwrap();
    assert_eq!((o.changed, o.protected), (1, 1));
    s.max_changes = 3;
    let before = s.tile(0, Y).unwrap();
    assert!(s.fill(Rect { x: 0, y: Y, w: 5, h: 1 }, Brush::front(7)).is_err());
    assert_eq!(s.tile(0, Y).unwrap(), before, "a refused change is rolled back");
    assert_eq!(s.history().0.len(), 1);
}

#[test]
fn copy_cut_paste() {
    let mut s = session();
    s.paint(OpKind::Pencil, &[(0, Y)], Brush::front(6), false).unwrap();
    let r = Rect { x: 0, y: Y, w: 3, h: 2 };
    let sch = s.copy(r).unwrap();
    assert_eq!(sch.present(), 6);
    assert_eq!(sch.meta.source_world.as_deref(), Some("TEST"));
    assert_eq!(sch.cell(0, 0).fg, 6);
    let edge = s.copy(Rect { x: 62, y: Y, w: 4, h: 1 }).unwrap();
    assert_eq!(edge.present(), 2, "ungenerated tiles are absent");
    assert!(s.copy(Rect { x: 0, y: 0, w: 0, h: 0 }).is_err());
    // paste into the air block, skipping air cells
    let mut named = sch.clone();
    named.meta.name = "Corner".into();
    let o = s.paste(&named, 40, Y, true).unwrap();
    assert_eq!(o.changed, 6);
    assert_eq!(s.tile(40, Y).unwrap().unwrap().fg(), 6);
    assert_eq!(s.history().0.last().unwrap().a.as_deref(), Some("Corner"));
    let (cut, o) = s.cut(Rect { x: 40, y: Y, w: 3, h: 2 }).unwrap();
    assert_eq!((cut.present(), o.changed), (6, 6));
    assert_eq!(s.tile(41, Y).unwrap().unwrap().fg(), 2);
    // an air-only schematic pastes nothing when skipping air
    let air = s.copy(Rect { x: 50, y: Y, w: 2, h: 1 }).unwrap();
    assert_eq!(s.paste(&air, 0, Y, true).unwrap().changed, 0);
    assert_eq!(s.paste(&air, 0, Y, false).unwrap().changed, 2);
    assert_eq!(s.history().0.last().unwrap().a, None);
}

#[test]
fn players_and_settings() {
    let mut s = session();
    open_column(&mut s, 10, 530);
    // put inside the rock: pushed up to the first free tile; the camera follows
    assert_eq!(s.move_blockhead("", 9, 10, 520).unwrap(), (10, 530));
    let cam = s.world.record.as_ref().unwrap().dict.get("translation").cloned();
    assert_eq!(cam, Some(Value::Array(vec![Value::Real(10.5), Value::Real(530.0)])));
    assert!(s.move_blockhead("", 9, 1000, 600).is_err());
    assert!(s.move_blockhead("", 77, 1, 1).is_err());
    assert_eq!(s.world.players.list()[0].pos, (10, 530));
    assert_eq!(s.history().0[0].a.as_deref(), Some("ADA"));
    assert!(s.changed_tiles().is_empty(), "no tiles in a blockhead step");
    s.set_stats("", 9, Stats { health: 2.0, happiness: -1.0, fullness: f32::NAN, energy: 0.5 }).unwrap();
    let st = s.world.players.list()[0].stats.unwrap();
    assert_eq!((st.health, st.happiness, st.fullness, st.energy), (1.0, 0.0, 0.0, 0.5));
    s.set_stack("", 9, 1, 21, 5).unwrap();
    assert_eq!(s.world.players.inventory("", 9).unwrap().slots[1].len(), 5);
    assert_eq!(s.history().0[2].b.as_deref(), Some("i:21"));
    s.set_stack("", 9, 1, 0, 0).unwrap();
    assert!(s.set_stack("", 9, 1, 21, 1000).is_err());
    assert!(s.set_stack("", 9, 1, 0, 1).is_err());
    assert!(s.set_stack("", 9, 99, 21, 1).is_err());
    assert!(s.set_stack("", 1, 0, 21, 1).is_err());
    s.set_name("new name").unwrap();
    s.set_name("NEW NAME").unwrap(); // same value: no step
    s.set_time_of_day(6.0).unwrap();
    assert_eq!(s.history().0.len(), 6);
    assert!(s.set_name("").is_err());
    s.go_to(0).unwrap();
    assert_eq!(s.world.players.list()[0].pos, (40, 530));
    assert_eq!(s.world.players.inventory("", 9).unwrap().slots[1].len(), 0);
    assert_eq!(s.world.record.as_ref().unwrap().name(), Some("TEST"));
    assert!(!s.world.record.as_ref().unwrap().dict.contains_key("translation"), "camera back");
    s.go_to(6).unwrap();
    assert_eq!(s.world.record.as_ref().unwrap().name(), Some("NEW NAME"));
    assert_eq!(s.world.players.list()[0].pos, (10, 530));
    // a world without a record has no settings
    let mut env = world_env();
    env.named.get_mut(&b"main"[..]).unwrap().entries.remove(0);
    let mut bare = Session::new(WorldContent::load(env).unwrap());
    assert!(bare.set_name("x").is_err());
    assert!(bare.set_time_of_day(1.0).is_err());
    assert!(s.set_stats("", 77, Stats { health: 1.0, happiness: 1.0, fullness: 1.0, energy: 1.0 }).is_err());
}

/// Makes column `x` air from row `y` to the top of the stored rock block (no history step).
fn open_column(s: &mut Session, x: u32, y: u32) {
    for y in y..Y + 32 {
        let t = Brush::eraser().apply(&s.world.blocks.tile(x, y).unwrap().unwrap());
        s.world.blocks.set_tile(x, y, t).unwrap();
    }
}

#[test]
fn blockhead_spots() {
    let mut s = session();
    open_column(&mut s, 10, 530);
    // put in the air: drops to the ground
    assert_eq!(s.move_blockhead("", 9, 10, 540).unwrap(), (10, 530));
    // water and lava: pushed up out of water; never above lava
    let mut water = s.world.blocks.tile(10, 530).unwrap().unwrap();
    water.set_fg(crate::brush::WATER);
    s.world.blocks.set_tile(10, 530, water).unwrap();
    assert_eq!(s.move_blockhead("", 9, 10, 530).unwrap(), (10, 531));
    assert_eq!(s.move_blockhead("", 9, 10, 540).unwrap(), (10, 531));
    water.set_fg(crate::brush::LAVA);
    s.world.blocks.set_tile(10, 530, water).unwrap();
    assert!(s.move_blockhead("", 9, 10, 540).unwrap_err().to_string().contains("lava"));
    // falling out of the stored blocks, or rising out of them, is refused
    assert!(s.move_blockhead("", 9, 40, 530).unwrap_err().to_string().contains("generated"));
    assert!(s.move_blockhead("", 9, 5, 520).unwrap_err().to_string().contains("generated"));
    assert!(s.move_blockhead("", 9, 100, 520).unwrap_err().to_string().contains("generated"));
    // solid up to the top of the world
    s.options = EditOptions { generate: true, objects: false };
    s.paint(OpKind::Pencil, &[(5, 1023)], Brush::front(1), false).unwrap();
    assert!(s.move_blockhead("", 9, 5, 1023).unwrap_err().to_string().contains("free"));
    // without a world record the camera is left alone
    let mut env = world_env();
    env.named.get_mut(&b"main"[..]).unwrap().entries.remove(0);
    let mut bare = Session::new(WorldContent::load(env).unwrap());
    open_column(&mut bare, 10, 530);
    assert_eq!(bare.move_blockhead("", 9, 10, 535).unwrap(), (10, 530));
    assert_eq!(bare.history().0.len(), 1);
}

#[test]
fn generate_and_objects() {
    let mut s = session();
    // ungenerated: refused by default, created when allowed (filled with the world's sky)
    let o = s.paint(OpKind::Pencil, &[(70, Y)], Brush::front(6), false).unwrap();
    assert_eq!(o.missing, 1);
    s.options = EditOptions { generate: true, objects: false };
    // painting air into an ungenerated area creates nothing
    assert_eq!(s.paint(OpKind::Pencil, &[(70, Y)], Brush::front(2), false).unwrap(), Outcome::default());
    let o = s.paint(OpKind::Pencil, &[(70, Y), (71, Y), (5000, Y)], Brush::front(6), false).unwrap();
    assert_eq!((o.changed, o.created, o.missing), (2, 1, 1));
    assert_eq!(s.tile(70, Y).unwrap().unwrap().fg(), 6);
    assert_eq!(s.tile(75, Y).unwrap().unwrap().fg(), 2);
    s.take_touched();
    s.undo().unwrap();
    assert_eq!(s.take_touched(), vec![(2, 16)]);
    assert_eq!(s.tile(70, Y).unwrap(), None);
    s.redo().unwrap();
    assert_eq!(s.tile(71, Y).unwrap().unwrap().fg(), 6);
    let env = s.save().unwrap();
    assert!(env.named[&b"blocks"[..]].entries.iter().any(|(k, _)| k == b"2_16"));

    // objects: a tree owning two tiles, and the chest standing on tile (3, 520)
    let mut s = session();
    for (x, y, c) in [(10, Y, 4u8), (11, Y + 1, 3)] {
        let mut t = s.tile(x, y).unwrap().unwrap();
        t.set_contents(c);
        t.0[0x28] = 5;
        s.world.blocks.set_tile(x, y, t).unwrap();
    }
    let o = s.paint(OpKind::Pencil, &[(10, Y), (3, 520)], Brush::front(6), false).unwrap();
    assert_eq!((o.changed, o.protected), (0, 2));
    s.options.objects = true;
    // a second object standing on the same tile goes too
    let plant = bhw_content::fixtures::object(6, 10, Y as i64).into_dictionary().unwrap();
    s.world.objects.insert("0_16/10", 0, plant).unwrap();
    let o = s.paint(OpKind::Pencil, &[(10, Y)], Brush::front(6), false).unwrap();
    assert_eq!((o.changed, o.removed), (2, 2));
    let t = s.tile(11, Y + 1).unwrap().unwrap();
    assert_eq!((t.contents(), t.owner()), (0, 0));
    let t = s.tile(10, Y).unwrap().unwrap();
    assert_eq!((t.fg(), t.contents(), t.owner()), (6, 0, 0));
    assert!(s.world.objects.all().0.is_empty());
    s.undo().unwrap();
    assert_eq!(s.world.objects.all().0.len(), 2);
    assert_eq!(s.tile(11, Y + 1).unwrap().unwrap().owner(), 5);
    s.redo().unwrap();
    assert!(s.world.objects.all().0.is_empty());
    s.undo().unwrap();
    // painting air over the chest's tile leaves it; a solid tile removes it
    assert_eq!(s.paint(OpKind::Remove, &[(3, 520)], Brush::eraser(), false).unwrap().removed, 0);
    let o = s.paint(OpKind::Pencil, &[(3, 520), (4, 520)], Brush::front(6), false).unwrap();
    assert_eq!((o.changed, o.removed), (4, 1), "the chest's owned tiles are cleared too");
    let saved = s.save().unwrap();
    assert_eq!(saved.named[&b"dw"[..]].entries.len(), 1, "the plant's record stays");

    // erasing a tree tile (an open tile: the object list is not needed) clears an owned plain tile too
    let mut s = session();
    s.options.objects = true;
    for (x, c) in [(20u32, 4u8), (21, 0)] {
        let mut t = s.tile(x, Y).unwrap().unwrap();
        t.set_contents(c);
        t.0[0x28] = 5;
        s.world.blocks.set_tile(x, Y, t).unwrap();
    }
    let o = s.paint(OpKind::Remove, &[(20, Y)], Brush::eraser(), false).unwrap();
    assert_eq!((o.changed, o.removed), (2, 1));
    assert_eq!(s.tile(21, Y).unwrap().unwrap().owner(), 0);
}

#[test]
fn blockheads_and_containers() {
    let mut s = session();
    s.remove_blockhead("", 9).unwrap();
    assert!(s.remove_blockhead("", 9).is_err());
    assert!(s.world.players.list().is_empty());
    s.undo().unwrap();
    assert_eq!(s.world.players.list()[0].name, "ADA");
    assert!(s.world.players.inventory("", 9).is_some());
    s.redo().unwrap();
    assert!(s.world.players.inventory("", 9).is_none());
    assert_eq!(s.history().0[0].kind, OpKind::RemoveBlockhead);

    // a chest with its slots under "chest", and one with them at the top level
    let (all, _) = s.world.objects.all();
    let key = all[0].key.clone();
    let mut d = all[0].dict.clone();
    let slots = bhw_content::Inventory { slots: vec![vec![]; 4] }.to_value();
    let mut chest = plist::Dictionary::new();
    chest.insert("saveItemSlots".into(), slots.clone());
    d.insert("chest".into(), plist::Value::Dictionary(chest));
    s.world.objects.set(&key, 0, d).unwrap();
    assert_eq!(s.container(&key, 0).unwrap().unwrap().slots.len(), 4);
    s.set_container_stack(&key, 0, 1, 21, 3).unwrap();
    assert_eq!(s.container(&key, 0).unwrap().unwrap().slots[1].len(), 3);
    s.undo().unwrap();
    assert!(s.container(&key, 0).unwrap().unwrap().slots[1].is_empty());
    s.redo().unwrap();
    assert_eq!(s.history().0.last().unwrap().b.as_deref(), Some("i:21"));
    let mut top = plist::Dictionary::new();
    top.insert("saveItemSlots".into(), slots);
    s.world.objects.insert(&key, 1, top).unwrap();
    s.set_container_stack(&key, 1, 0, 21, 1).unwrap();
    assert_eq!(s.container(&key, 1).unwrap().unwrap().slots[0].len(), 1);
    // errors: no such object, no slots, bad stacks
    assert!(s.container(&key, 9).is_err());
    s.world.objects.insert(&key, 2, plist::Dictionary::new()).unwrap();
    assert_eq!(s.container(&key, 2).unwrap(), None);
    assert!(s.set_container_stack(&key, 2, 0, 21, 1).is_err());
    assert!(s.set_container_stack(&key, 1, 0, 21, 1000).is_err());
    assert!(s.set_container_stack(&key, 1, 0, 0, 1).is_err());
}

#[test]
fn refusal_reasons() {
    let why = |e: crate::Error| e.refusal().unwrap();
    let mut s = session();
    let r = Rect { x: 0, y: Y, w: 2, h: 1 };
    assert_eq!(why(s.fill(Rect { x: 0, y: 0, w: 0, h: 1 }, Brush::front(6)).unwrap_err()), Refusal::AreaSize);
    assert_eq!(why(s.fill(r, Brush::front(250)).unwrap_err()), Refusal::UnknownBlock);
    let oil_in_rock = Brush { layer: Layer::Front, tile: 1, contents: 64 };
    assert_eq!(why(s.fill(r, oil_in_rock).unwrap_err()), Refusal::MaterialNotInTile);
    assert!(s.fill(r, Brush { contents: 0, ..oil_in_rock }).is_ok());
    assert_eq!(why(s.move_blockhead("", 9, 1000, 600).unwrap_err()), Refusal::OutsideWorld);
    assert_eq!(why(s.move_blockhead("", 77, 1, Y + 40).unwrap_err()), Refusal::NotGenerated);
    open_column(&mut s, 10, 530);
    assert_eq!(why(s.move_blockhead("", 77, 10, 530).unwrap_err()), Refusal::NoBlockhead);
    assert_eq!(why(s.set_stack("", 9, 1, 21, 1000).unwrap_err()), Refusal::StackTooLarge);
    assert_eq!(why(s.set_stack("", 9, 1, 0, 1).unwrap_err()), Refusal::UnknownItem);
    assert_eq!(why(s.set_stack("", 1, 0, 21, 1).unwrap_err()), Refusal::NoInventory);
    assert_eq!(why(s.set_stack("", 9, 99, 21, 1).unwrap_err()), Refusal::Other);
    assert_eq!(why(s.set_name(" ").unwrap_err()), Refusal::BadName);
    assert_eq!(why(s.go_to(99).unwrap_err()), Refusal::NoStep);
    assert_eq!(why(s.set_container_stack("nope", 0, 0, 21, 1).unwrap_err()), Refusal::NoObject);
    let damaged = crate::Error::Content(bhw_content::Error::Format("x".into()));
    assert_eq!(damaged.refusal(), None);
}
