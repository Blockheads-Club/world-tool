//! Pure-Rust reader and writer for the LMDB data file format (`data.mdb`).
//!
//! Supports both on-disk layouts that occur with The Blockheads:
//!
//! * [`Layout::Bits32`]: written by the game itself (Android ARMv7, PC); 12-byte page headers.
//! * [`Layout::Bits64`]: written by Blockheads Server on x86_64; 16-byte page headers.
//!
//! The crate works on byte slices only (no mmap, no `unsafe`, no C). [`Env::open`] parses a file image,
//! [`write_env`] produces a fresh compact image (like `mdb_copy -c`) from [`EnvContents`], and [`convert`]
//! re-encodes an image in the other layout. Lock files (`lock.mdb`) are not handled: they are recreated by LMDB
//! and must not be copied between layouts.
//!
//! Struct layouts are documented against LMDB's `mdb.c` in [`layout`] and [`format`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
pub mod format;
pub mod layout;
mod model;
mod reader;
mod writer;

#[cfg(test)]
mod tests;

pub use error::{Error, Result};
pub use format::{DbFlags, DbRecord, Meta};
pub use layout::Layout;
pub use model::{Database, EnvContents};
pub use reader::{read_file, Env, Iter};
pub use writer::{convert, write_env};
