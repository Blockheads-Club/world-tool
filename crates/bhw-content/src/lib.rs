//! World contents of The Blockheads saves for BH World Tool's editor: terrain tiles (`blocks`), dynamic objects
//! (`dw`), blockheads and their inventories, and the world record (`main`).
//!
//! [`WorldContent`] loads a `world_db` ([`bhw_lmdb::EnvContents`]) of a game client or Blockheads Server world,
//! lets the caller read and change it, and writes back only the records that changed: every other record keeps its
//! original bytes. The format is described in `docs/client-save-format.md` (§3–5) and the module docs here.

pub mod blocks;
pub mod error;
pub mod names;
mod names_gen;
pub mod objects;
pub mod players;
pub mod record;
pub mod stored;
pub mod tile;
pub mod world;

pub use blocks::BlockStore;
pub use error::{Error, Result};
pub use objects::{DynObject, Objects};
pub use players::{Blockhead, Inventory, Item, Players, Stats};
pub use record::WorldRecord;
pub use tile::TileData;
pub use world::WorldContent;
