//! World identity, conflict helpers and the plug-in points for LMDB-backed metadata.
//!
//! The world's display name, game version etc. are stored inside `world_db/data.mdb` (32-bit LMDB). Reading and
//! writing them needs `bhw-lmdb`, which is developed separately; this crate only defines the traits
//! ([`WorldMetaSource`], [`WorldMetaEditor`], [`ServerWorldConverter`]) and works without them (names then come from
//! `.bhworld` manifests or the user).

use crate::files::WorldFiles;
use crate::Result;

/// Metadata of a world as far as it is known.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldMeta {
    /// Display name.
    pub name: Option<String>,
    /// Game version that last saved the world (e.g. `1.7.6`).
    pub game_version: Option<String>,
    /// Creation time, RFC 3339.
    pub created_at: Option<String>,
}

/// Reads [`WorldMeta`] from a world's files (implemented with `bhw-lmdb`).
pub trait WorldMetaSource {
    /// Reads the metadata.
    fn read_meta(&self, world: &WorldFiles) -> Result<WorldMeta>;
}

/// Changes a world's identity inside its databases (implemented with `bhw-lmdb`).
///
/// TODO(bhw-lmdb): find out whether the save id is also stored inside `world_db` (then a "keep both" copy must
/// rewrite it) and where the display name lives.
pub trait WorldMetaEditor {
    /// Sets the stored name and/or id; `world.id` itself is changed by the caller.
    fn set_identity(&self, world: &mut WorldFiles, new_id: Option<&str>, new_name: Option<&str>) -> Result<()>;
}

/// Converts between Blockheads Server worlds (64-bit LMDB, extra `server_db/` and `lightBlocks/`) and game worlds
/// (32-bit LMDB). TODO(bhw-lmdb): implement once the LMDB reader/writer exists.
pub trait ServerWorldConverter {
    /// Server world → game world.
    fn server_to_game(&self, world: &WorldFiles) -> Result<WorldFiles>;
    /// Game world → server world.
    fn game_to_server(&self, world: &WorldFiles) -> Result<WorldFiles>;
}

/// A [`WorldMetaSource`] that knows nothing (used until `bhw-lmdb` is plugged in).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoMeta;

impl WorldMetaSource for NoMeta {
    fn read_meta(&self, _world: &WorldFiles) -> Result<WorldMeta> {
        Ok(WorldMeta::default())
    }
}

/// A fresh random world id (32 lowercase hex digits, like the game's).
pub fn new_world_id() -> String {
    let mut b = [0u8; 16];
    bhw_backup::random_bytes(&mut b);
    bhw_backup::hex::encode_lower(&b)
}

/// Id and name of a world, for conflict checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSummary {
    /// World id.
    pub id: String,
    /// Display name if known.
    pub name: Option<String>,
}

/// Indexes of existing worlds that conflict with an incoming one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conflicts {
    /// Same id (would overwrite the existing world's folder).
    pub same_id: Vec<usize>,
    /// Same name (case-insensitive, trimmed), different id.
    pub same_name: Vec<usize>,
}

impl Conflicts {
    /// `true` if nothing conflicts.
    pub fn is_empty(&self) -> bool {
        self.same_id.is_empty() && self.same_name.is_empty()
    }
}

fn name_key(n: &str) -> String {
    n.trim().to_lowercase()
}

/// Finds worlds in `existing` that have the same id or name as `incoming`.
pub fn find_conflicts(incoming: &WorldSummary, existing: &[WorldSummary]) -> Conflicts {
    let mut c = Conflicts::default();
    let key = incoming.name.as_deref().map(name_key);
    for (i, e) in existing.iter().enumerate() {
        if e.id == incoming.id {
            c.same_id.push(i);
        } else if key.is_some() && e.name.as_deref().map(name_key) == key {
            c.same_name.push(i);
        }
    }
    c
}

/// Splits `"Name (3)"` into `("Name", 3)`; names without a counter give `(name, 1)`.
fn split_counter(name: &str) -> (&str, u32) {
    let t = name.trim_end();
    if let Some(open) = t.strip_suffix(')').and_then(|s| s.rfind(" (")) {
        if let Ok(n) = t[open + 2..t.len() - 1].parse::<u32>() {
            if n >= 2 {
                return (&t[..open], n);
            }
        }
    }
    (t, 1)
}

/// `name` if no name in `taken` equals it (case-insensitive), else `"Name (2)"`, `"Name (3)"`, ...
pub fn unique_name(name: &str, taken: &[&str]) -> String {
    let keys: Vec<String> = taken.iter().map(|t| name_key(t)).collect();
    if !keys.contains(&name_key(name)) {
        return name.to_string();
    }
    let (base, n) = split_counter(name);
    (n + 1..).map(|i| format!("{base} ({i})")).find(|c| !keys.contains(&name_key(c))).expect("an unused counter exists")
}

/// "Keep both": a new random id that is not in `existing` and a unique name (`"Name (2)"`).
pub fn keep_both(incoming: &WorldSummary, existing: &[WorldSummary]) -> WorldSummary {
    let id = loop {
        let id = new_world_id();
        if !existing.iter().any(|e| e.id == id) {
            break id;
        }
    };
    let taken: Vec<&str> = existing.iter().filter_map(|e| e.name.as_deref()).collect();
    WorldSummary { id, name: incoming.name.as_deref().map(|n| unique_name(n, &taken)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: &str, name: Option<&str>) -> WorldSummary {
        WorldSummary { id: id.into(), name: name.map(str::to_string) }
    }

    #[test]
    fn ids_are_random_hex() {
        let a = new_world_id();
        assert!(bhw_backup::saves::is_world_id(&a));
        assert_ne!(a, new_world_id());
    }

    #[test]
    fn conflicts() {
        let existing = [w("a", Some("Home")), w("b", Some(" home ")), w("c", None), w("d", Some("Other"))];
        let c = find_conflicts(&w("a", Some("HOME")), &existing);
        assert_eq!(c, Conflicts { same_id: vec![0], same_name: vec![1] });
        assert!(!c.is_empty());
        assert!(find_conflicts(&w("x", None), &existing).is_empty());
        assert!(find_conflicts(&w("x", Some("new")), &existing).is_empty());
    }

    #[test]
    fn unique_names() {
        assert_eq!(unique_name("World", &[]), "World");
        assert_eq!(unique_name("World", &["world"]), "World (2)");
        assert_eq!(unique_name("World", &["World", "World (2)"]), "World (3)");
        assert_eq!(unique_name("World (2)", &["World (2)"]), "World (3)");
        assert_eq!(unique_name("World (1)", &["World (1)"]), "World (1) (2)");
        assert_eq!(unique_name("W (x)", &["W (x)"]), "W (x) (2)");
        assert_eq!(split_counter("A (7) "), ("A", 7));
    }

    #[test]
    fn keep_both_makes_new_identity() {
        let existing = [w("a", Some("Home")), w("b", Some("Home (2)"))];
        let k = keep_both(&w("a", Some("Home")), &existing);
        assert_ne!(k.id, "a");
        assert_eq!(k.name.as_deref(), Some("Home (3)"));
        assert_eq!(keep_both(&w("a", None), &existing).name, None);
    }

    #[test]
    fn no_meta_knows_nothing() {
        let world = WorldFiles::new("a".repeat(32), vec![]).unwrap();
        assert_eq!(NoMeta.read_meta(&world).unwrap(), WorldMeta::default());
    }
}
