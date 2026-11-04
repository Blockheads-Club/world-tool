//! Edit model of BH World Tool's world editor.
//!
//! A [`Session`] holds one world's contents ([`bhw_content::WorldContent`]) and its history. Every change is an
//! operation (pencil, brush, remove, fill, replace, paste, player and settings edits) recorded as one undoable step
//! holding the before/after values it touched. Selections are copied into [`Schematic`]s, which can be saved as
//! files (`docs/schematic-format.md`). [`view`] produces compact per-block tile data and overview images for the
//! map renderer; [`atlas`] maps ids to cells of the game's own atlases (extracted from the user's APK).

pub mod atlas;
pub mod brush;
pub mod error;
pub mod geom;
pub mod history;
pub mod schematic;
pub mod session;
pub mod view;

pub use brush::{Brush, Layer, Match};
pub use error::{Error, Result};
pub use geom::Rect;
pub use history::{Label, OpKind};
pub use schematic::Schematic;
pub use session::{Outcome, Session};
