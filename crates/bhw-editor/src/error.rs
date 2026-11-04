//! Error type.

use serde::Serialize;

/// Why the editor refused a request. The UI shows a translated message for each (`edv.refused.<code>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// One step would change more tiles than a step may hold.
    TooManyChanges,
    /// A rectangle is empty or larger than the editor allows.
    AreaSize,
    /// The block (tile type) is not known.
    UnknownBlock,
    /// The material does not belong in that block (flint and clay live in dirt, oil in limestone, ores in rock,
    /// gems in cave air).
    MaterialNotInTile,
    /// The blockhead does not exist (any more).
    NoBlockhead,
    /// The position lies outside the world.
    OutsideWorld,
    /// The game has not generated that area yet.
    NotGenerated,
    /// There is no free tile above the spot to stand on.
    NoFreeTile,
    /// The spot is above lava.
    Lava,
    /// The blockhead has no inventory.
    NoInventory,
    /// The object does not exist (any more).
    NoObject,
    /// The object holds no items.
    NoItems,
    /// The world has no world record (name, time…).
    NoRecord,
    /// The history has no such step.
    NoStep,
    /// A stack holds at most 999 items.
    StackTooLarge,
    /// The item is not known.
    UnknownItem,
    /// A world name needs 1 to 64 characters.
    BadName,
    /// Too many blocks were asked for at once.
    RegionTooLarge,
    /// A schematic file is not valid.
    BadSchematic,
    /// A schematic file was made by a newer version of BH World Tool.
    NewerSchematic,
    /// Anything else the world's contents refuse.
    Other,
}

/// Errors of this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The world's contents could not be read or written.
    #[error(transparent)]
    Content(#[from] bhw_content::Error),
    /// The request is not possible; `detail` is the technical line.
    #[error("{detail}")]
    Refused { reason: Refusal, detail: String },
}

/// A refused request.
pub fn refused(reason: Refusal, detail: impl Into<String>) -> Error {
    Error::Refused { reason, detail: detail.into() }
}

impl Error {
    /// Why the request was refused (`Other` for errors of the world's contents that are not damage).
    pub fn refusal(&self) -> Option<Refusal> {
        match self {
            Error::Refused { reason, .. } => Some(*reason),
            Error::Content(bhw_content::Error::Invalid(_)) => Some(Refusal::Other),
            Error::Content(_) => None,
        }
    }
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;
