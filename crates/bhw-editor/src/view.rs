//! Data for the map renderer: compact tile data per macro block and a whole-world overview image.

use bhw_content::blocks::{MacroPos, WORLD_HEIGHT};
use bhw_content::tile::{TileData, TILE_BYTES};
use bhw_save::blocks::{Tile, MACRO_SIZE, WORLD_HEIGHT_MACROS};
use bhw_save::png::RgbaImage;
use bhw_save::preview::tile_color;

use crate::error::Result;
use crate::session::Session;

/// Bytes per tile in [`block_view`]: foreground, back wall, material ([`crate::brush::material`]), light (the brighter of light and sun light).
pub const VIEW_BYTES: usize = 4;
/// Most macro blocks one [`region`] request may ask for.
pub const MAX_REGION_BLOCKS: u32 = 256;

/// The compact view of one stored block: 1024 tiles × [`VIEW_BYTES`], bottom row first; `None` if not stored.
pub fn block_view(s: &mut Session, pos: MacroPos) -> Result<Option<Vec<u8>>> {
    let Some(raw) = s.world.blocks.block_tiles(pos)? else { return Ok(None) };
    let mut out = Vec::with_capacity(1024 * VIEW_BYTES);
    for t in raw.chunks_exact(TILE_BYTES) {
        let m = crate::brush::material(&TileData::from_slice(t));
        out.extend_from_slice(&[t[0], t[1], m, t[6].max(t[7])]);
    }
    Ok(Some(out))
}

/// The blocks of a rectangle of macro positions `[mx0, mx1) × [my0, my1)` as one binary message: for each
/// position (row by row from the bottom) `u16 mx, u8 my, u8 state` (0 = not stored, 1 = stored, 2 = damaged),
/// followed by the [`block_view`] bytes when stored. Positions outside the world are left out.
pub fn region(s: &mut Session, mx0: u32, my0: u32, mx1: u32, my1: u32) -> Result<Vec<u8>> {
    let wm = s.width() / MACRO_SIZE;
    let (mx1, my1) = (mx1.min(wm), my1.min(WORLD_HEIGHT_MACROS));
    if mx1.saturating_sub(mx0) * my1.saturating_sub(my0) > MAX_REGION_BLOCKS {
        return Err(crate::error::refused(
            crate::error::Refusal::RegionTooLarge,
            format!("at most {MAX_REGION_BLOCKS} blocks per request"),
        ));
    }
    let mut out = Vec::new();
    for my in my0..my1 {
        for mx in mx0..mx1 {
            out.extend_from_slice(&(mx as u16).to_le_bytes());
            out.push(my as u8);
            match block_view(s, (mx, my)) {
                Ok(Some(v)) => {
                    out.push(1);
                    out.extend_from_slice(&v);
                }
                Ok(None) => out.push(0),
                Err(_) => out.push(2),
            }
        }
    }
    Ok(out)
}

/// The whole world drawn with plain colours, one pixel per `scale` × `scale` tiles (the tile at the square's
/// centre); top row first. Areas that are not stored are transparent.
pub fn overview(s: &mut Session, scale: u32) -> RgbaImage {
    let n = scale.max(1);
    let (w, h) = (s.width(), WORLD_HEIGHT);
    let mut img = RgbaImage::new(w.div_ceil(n), h.div_ceil(n));
    let positions: Vec<MacroPos> = s.world.blocks.positions().filter(|p| p.0 * MACRO_SIZE < w).collect();
    for (mx, my) in positions {
        let Ok(Some(raw)) = s.world.blocks.block_tiles((mx, my)) else { continue };
        let (x0, y0) = (mx * MACRO_SIZE, my * MACRO_SIZE);
        // pixels whose sample tile lies in this block
        for py in (h - (y0 + MACRO_SIZE)).div_ceil(n)..(h - y0).div_ceil(n) {
            let ty = h - 1 - (py * n + n / 2).min(h - 1);
            if ty < y0 || ty >= y0 + MACRO_SIZE {
                continue;
            }
            for px in x0.div_ceil(n)..(x0 + MACRO_SIZE).div_ceil(n) {
                let tx = (px * n + n / 2).min(w - 1);
                if tx < x0 || tx >= x0 + MACRO_SIZE {
                    continue;
                }
                let i = (((ty - y0) * MACRO_SIZE + (tx - x0)) as usize) * TILE_BYTES;
                img.put(px, py, tile_color(Tile(&raw[i..i + TILE_BYTES])));
            }
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhw_content::fixtures::world_env;
    use bhw_content::WorldContent;

    fn session() -> Session {
        Session::new(WorldContent::load(world_env()).unwrap())
    }

    #[test]
    fn views() {
        let mut s = session();
        let v = block_view(&mut s, (0, 16)).unwrap().unwrap();
        assert_eq!(v.len(), 4096);
        assert_eq!(&v[..4], &[1, 1, 0, 0]);
        assert!(block_view(&mut s, (3, 3)).unwrap().is_none());
        let r = region(&mut s, 0, 16, 9, 17).unwrap();
        // 4 positions inside the world: 2 stored, 2 not
        assert_eq!(r.len(), 4 * 4 + 2 * 4096);
        assert_eq!(&r[..4], &[0, 0, 16, 1]);
        assert!(region(&mut s, 0, 0, 4, 32).is_ok());
        let mut big = session();
        big.world.record.as_mut().unwrap().dict.insert("worldWidthMacro".into(), plist::Value::Integer(100.into()));
        assert!(region(&mut big, 0, 0, 100, 32).is_err());
    }

    #[test]
    fn damaged_blocks() {
        let mut env = world_env();
        env.named.get_mut(&b"blocks"[..]).unwrap().entries.push((b"2_16".to_vec(), b"bad".to_vec()));
        let mut s = Session::new(WorldContent::load(env).unwrap());
        let r = region(&mut s, 2, 16, 3, 17).unwrap();
        assert_eq!(r, vec![2, 0, 16, 2]);
        let img = overview(&mut s, 1);
        assert_eq!(img.get(64, 1023 - 512)[3], 0, "damaged block left transparent");
    }

    #[test]
    fn overviews() {
        let mut s = session();
        let img = overview(&mut s, 1);
        assert_eq!((img.width, img.height), (128, 1024));
        // tile (0, 512) is rock, (40, 512) air, (100, 512) not stored
        assert_eq!(img.get(0, 1023 - 512), [110, 110, 115, 255]);
        assert_eq!(img.get(40, 1023 - 512), [150, 200, 240, 255]);
        assert_eq!(img.get(100, 1023 - 512)[3], 0);
        let small = overview(&mut s, 4);
        assert_eq!((small.width, small.height), (32, 256));
        assert_eq!(small.get(0, (1023 - 513) / 4)[3], 255);
        let odd = overview(&mut s, 7);
        assert_eq!(odd.width, 19);
        assert!(odd.pixels.chunks(4).any(|p| p[3] == 255));
        assert_eq!(overview(&mut s, 0).width, 128);
    }
}
