//! Undo/redo history: a list of steps, each the before/after values of everything one operation changed, and a
//! cursor (the number of applied steps). New steps after an undo drop the undone ones.

use bhw_content::players::Inventory;
use bhw_content::TileData;
use plist::Dictionary;
use serde::{Deserialize, Serialize};

/// What an operation was (the UI turns it into a translated label).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// Single tiles drawn with the pencil.
    Pencil,
    /// Brush strokes.
    Brush,
    /// Tiles removed (eraser).
    Remove,
    /// A rectangle filled.
    Fill,
    /// A selection cleared.
    Clear,
    /// Tiles of one kind replaced with another.
    Replace,
    /// A schematic or clipboard pasted.
    Paste,
    /// A selection cut.
    Cut,
    /// A blockhead moved or its needs changed.
    Blockhead,
    /// An inventory changed.
    Inventory,
    /// World settings changed.
    Settings,
}

/// The description of a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// Operation.
    pub kind: OpKind,
    /// Tiles changed (0 for non-tile operations).
    pub tiles: u64,
    /// First detail (brush, replaced block, schematic or blockhead name…).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    /// Second detail (replacement block…).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
}

/// One changed value.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A tile.
    Tile {
        /// Column.
        x: u32,
        /// Row.
        y: u32,
        /// Old record.
        before: TileData,
        /// New record.
        after: TileData,
    },
    /// A blockhead's stored dictionary.
    Blockhead {
        /// Player id (`""` = single player).
        player: String,
        /// `uniqueID`.
        id: u64,
        /// Old dictionary.
        before: Dictionary,
        /// New dictionary.
        after: Dictionary,
    },
    /// An inventory.
    Inventory {
        /// Player id.
        player: String,
        /// Blockhead `uniqueID`.
        id: u64,
        /// Old inventory.
        before: Inventory,
        /// New inventory.
        after: Inventory,
    },
    /// A block created where the game had not generated one (undo removes it).
    BlockCreated {
        /// Macro block position.
        pos: (u32, u32),
        /// The tile the new block was filled with.
        fill: TileData,
    },
    /// A dynamic object removed (or put back).
    Object {
        /// Its `dw` record key.
        key: String,
        /// Index in the record.
        index: usize,
        /// The object's dictionary (`Some` = present before).
        before: Option<Dictionary>,
        /// After the change.
        after: Option<Dictionary>,
    },
    /// The world record's dictionary.
    Record {
        /// Old dictionary.
        before: Dictionary,
        /// New dictionary.
        after: Dictionary,
    },
}

/// One undoable step.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Description.
    pub label: Label,
    /// Changes in the order they were made.
    pub changes: Vec<Change>,
}

/// The history.
#[derive(Debug, Clone)]
pub struct History {
    steps: Vec<Step>,
    cursor: usize,
    /// Cursor value at the last save (`None` if that state was dropped from the history).
    saved_at: Option<usize>,
}

impl Default for History {
    fn default() -> History {
        History::new()
    }
}

impl History {
    /// An empty history whose state counts as saved.
    pub fn new() -> History {
        History { steps: Vec::new(), cursor: 0, saved_at: Some(0) }
    }

    /// Adds a step after the cursor, dropping undone steps.
    pub fn push(&mut self, step: Step) {
        self.steps.truncate(self.cursor);
        if self.saved_at.is_some_and(|s| s > self.cursor) {
            self.saved_at = None;
        }
        self.steps.push(step);
        self.cursor += 1;
    }

    /// The step to undo, moving the cursor back.
    pub fn undo(&mut self) -> Option<&Step> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.steps.get(self.cursor)
    }

    /// The step to redo, moving the cursor forward.
    pub fn redo(&mut self) -> Option<&Step> {
        let s = self.steps.get(self.cursor)?;
        self.cursor += 1;
        Some(s)
    }

    /// Number of applied steps.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// All steps (applied ones first `cursor()` entries).
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Marks the current state as saved.
    pub fn mark_saved(&mut self) {
        self.saved_at = Some(self.cursor);
    }

    /// Number of steps between the saved state and now (the unsaved-changes count).
    pub fn unsaved(&self) -> usize {
        match self.saved_at {
            Some(s) => s.abs_diff(self.cursor),
            None => self.cursor.max(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(n: u64) -> Step {
        Step { label: Label { kind: OpKind::Pencil, tiles: n, a: None, b: None }, changes: Vec::new() }
    }

    #[test]
    fn undo_redo() {
        let mut h = History::new();
        assert!(h.undo().is_none());
        assert!(h.redo().is_none());
        assert_eq!(h.unsaved(), 0);
        h.push(step(1));
        h.push(step(2));
        assert_eq!(h.unsaved(), 2);
        assert_eq!(h.undo().unwrap().label.tiles, 2);
        assert_eq!(h.cursor(), 1);
        assert_eq!(h.redo().unwrap().label.tiles, 2);
        h.mark_saved();
        assert_eq!(h.unsaved(), 0);
        h.undo();
        h.undo();
        assert_eq!(h.unsaved(), 2);
        h.push(step(3));
        assert_eq!(h.steps().len(), 1);
        assert_eq!(h.unsaved(), 1, "saved state is gone");
        h.undo();
        assert_eq!(h.unsaved(), 1, "even back at the start, the saved state is gone");
    }
}
