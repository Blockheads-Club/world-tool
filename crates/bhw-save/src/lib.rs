//! Contents of The Blockheads saves: world summaries, map previews, the game's world list (`game_db`), world
//! identity, and conversion between game client worlds and Blockheads Server worlds.
//!
//! The on-disk schema is described in `docs/client-save-format.md`. LMDB files are read and written with
//! [`bhw_lmdb`]; a world is handled either as a folder or in memory as a [`save::FileMap`].

pub mod blocks;
pub mod convert;
pub mod error;
pub mod gamedb;
pub mod identity;
pub mod plistio;
pub mod png;
pub mod preview;
pub mod save;
pub mod summary;
pub mod tiles;
pub mod world;

pub use convert::{client_to_server, server_to_client, ConversionReport, ConvertOptions, Note, NoteKind};
pub use error::{Error, Result};
pub use gamedb::GameDb;
pub use png::RgbaImage;
pub use preview::{render_map, stored_preview, MapOptions};
pub use save::{AnySave, ClientSave, FileMap, SaveKind, ServerSave};
pub use summary::{summarize, summarize_any, BlockheadInfo, WorldSummary};
