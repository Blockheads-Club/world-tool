//! Map previews drawn from the `blocks` database with this tool's own colours (no game graphics), and the
//! game's own thumbnail stored in the world record.

use bhw_lmdb::EnvContents;
use plist::Value;

use crate::blocks::{decode_all, MacroBlock, Tile, MACRO_SIZE, WORLD_HEIGHT_MACROS};
use crate::plistio::parse_dict;
use crate::png::RgbaImage;
use crate::save::get;
use crate::tiles::TileType;

/// Options for [`render_map`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapOptions {
    /// Tiles per pixel side (1 = one pixel per tile). Each pixel shows the tile at the centre of its square.
    pub tiles_per_pixel: u32,
    /// Crop the image to the stored blocks instead of drawing the whole world.
    pub crop_to_content: bool,
}

impl Default for MapOptions {
    fn default() -> MapOptions {
        MapOptions { tiles_per_pixel: 1, crop_to_content: true }
    }
}

/// Colour of areas that are not stored (never generated or loaded).
pub const UNKNOWN: [u8; 4] = [0, 0, 0, 0];
/// Colour of tiles with an id this tool does not know.
pub const UNKNOWN_TILE: [u8; 4] = [255, 0, 255, 255];

/// The preview colour of one tile: its foreground type, or for air in front of a back wall a darkened back wall.
pub fn tile_color(tile: Tile<'_>) -> [u8; 4] {
    let front = TileType(tile.tile_type());
    let back = TileType(tile.back_wall());
    if front == TileType::AIR && back != TileType::AIR && back.color().is_some() {
        let [r, g, b] = back.color().expect("checked");
        return [r / 2, g / 2, b / 2, 255];
    }
    match front.color() {
        Some([r, g, b]) => [r, g, b, 255],
        None => UNKNOWN_TILE,
    }
}

/// Renders decoded blocks. `world_width_macro` sets the image width when not cropping (blocks beyond it are
/// ignored); `y` grows upwards in the world and downwards in the image.
pub fn render_blocks(blocks: &[MacroBlock], world_width_macro: u32, opts: MapOptions) -> RgbaImage {
    let n = opts.tiles_per_pixel.max(1);
    let blocks: Vec<&MacroBlock> =
        blocks.iter().filter(|b| b.x < world_width_macro && b.y < WORLD_HEIGHT_MACROS).collect();
    // area in macro blocks: [x0, x1) × [y0, y1)
    let (x0, x1, y0, y1) = if opts.crop_to_content && !blocks.is_empty() {
        (
            blocks.iter().map(|b| b.x).min().expect("not empty"),
            blocks.iter().map(|b| b.x).max().expect("not empty") + 1,
            blocks.iter().map(|b| b.y).min().expect("not empty"),
            blocks.iter().map(|b| b.y).max().expect("not empty") + 1,
        )
    } else if opts.crop_to_content {
        (0, 0, 0, 0)
    } else {
        (0, world_width_macro, 0, WORLD_HEIGHT_MACROS)
    };
    let tiles_w = (x1 - x0) * MACRO_SIZE;
    let tiles_h = (y1 - y0) * MACRO_SIZE;
    let mut img = RgbaImage::new(tiles_w.div_ceil(n), tiles_h.div_ceil(n));
    let mut grid: Vec<Option<&MacroBlock>> = vec![None; ((x1 - x0) * (y1 - y0)) as usize];
    for b in blocks {
        grid[((b.y - y0) * (x1 - x0) + (b.x - x0)) as usize] = Some(b);
    }
    for py in 0..img.height {
        for px in 0..img.width {
            // tile coordinates relative to the area's bottom-left corner
            let tx = (px * n + n / 2).min(tiles_w - 1);
            let ty_from_top = (py * n + n / 2).min(tiles_h - 1);
            let ty = tiles_h - 1 - ty_from_top;
            let (mx, my) = (tx / MACRO_SIZE, ty / MACRO_SIZE);
            let color = match grid[(my * (x1 - x0) + mx) as usize] {
                Some(b) => tile_color(b.tile(tx % MACRO_SIZE, ty % MACRO_SIZE)),
                None => UNKNOWN,
            };
            img.put(px, py, color);
        }
    }
    img
}

/// World width in macro blocks from `main/worldv2`, else from the stored blocks.
pub fn world_width_macro(world_db: &EnvContents, blocks: &[MacroBlock]) -> u32 {
    get(world_db, "main", "worldv2")
        .and_then(|r| parse_dict(r).ok())
        .and_then(|d| d.get("worldWidthMacro").and_then(Value::as_signed_integer))
        .and_then(|w| u32::try_from(w).ok())
        .unwrap_or_else(|| blocks.iter().map(|b| b.x + 1).max().unwrap_or(0))
}

/// Renders the map of a `world_db`. Undecodable block records are left out (they are counted in the second
/// value).
pub fn render_map(world_db: &EnvContents, opts: MapOptions) -> (RgbaImage, usize) {
    let (blocks, bad) = match world_db.named.get(&b"blocks"[..]) {
        Some(db) => decode_all(&db.entries),
        None => (Vec::new(), Vec::new()),
    };
    let width = world_width_macro(world_db, &blocks);
    (render_blocks(&blocks, width, opts), bad.len())
}

/// Side length of the game's own thumbnail.
pub const STORED_PREVIEW_SIZE: u32 = 320;

/// Decodes the game's thumbnail (`previewImageData` in the world record: 320 × 320 RGBA, bottom row first).
/// Note: it is a screenshot the game took of the world, so it shows game graphics; only display it, never
/// redistribute it.
pub fn stored_preview(record: &[u8]) -> Option<RgbaImage> {
    let d = parse_dict(record).ok()?;
    let data = d.get("previewImageData")?.as_data()?;
    let row = STORED_PREVIEW_SIZE as usize * 4;
    if data.len() != row * STORED_PREVIEW_SIZE as usize {
        return None;
    }
    let mut pixels = Vec::with_capacity(data.len());
    for r in data.chunks_exact(row).rev() {
        pixels.extend_from_slice(r);
    }
    Some(RgbaImage { width: STORED_PREVIEW_SIZE, height: STORED_PREVIEW_SIZE, pixels })
}
