//! Tile rectangles and brush shapes. Coordinates are world tiles, `y = 0` at the bottom of the world.

use serde::{Deserialize, Serialize};

/// Largest number of tiles one operation may touch (a 2048 × 1024 area).
pub const MAX_TILES: u64 = 2048 * 1024;

/// A rectangle of tiles: `x..x + w` × `y..y + h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    /// Left column.
    pub x: u32,
    /// Bottom row.
    pub y: u32,
    /// Width in tiles.
    pub w: u32,
    /// Height in tiles.
    pub h: u32,
}

impl Rect {
    /// The rectangle spanned by two corner tiles (inclusive, any order).
    pub fn spanning(a: (u32, u32), b: (u32, u32)) -> Rect {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        Rect { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }
    }

    /// Number of tiles.
    pub fn area(&self) -> u64 {
        self.w as u64 * self.h as u64
    }

    /// True if `(x, y)` is inside.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x - self.x < self.w && y - self.y < self.h
    }

    /// All tiles, row by row from the bottom.
    pub fn tiles(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (0..self.h).flat_map(move |dy| (0..self.w).map(move |dx| (self.x + dx, self.y + dy)))
    }
}

/// The tiles a brush of `size` covers around `(cx, cy)` (round or square), inside `0..width` × `0..height`.
pub fn brush_points(cx: u32, cy: u32, size: u32, round: bool, width: u32, height: u32) -> Vec<(u32, u32)> {
    let size = size.clamp(1, 64) as i64;
    let lo = -(size - 1) / 2;
    let hi = size / 2;
    let r2 = (size as f64 / 2.0).powi(2);
    let mut out = Vec::new();
    for dy in lo..=hi {
        for dx in lo..=hi {
            if round && size > 2 {
                // distance from the brush centre (between tiles for even sizes)
                let off = if size % 2 == 0 { 0.5 } else { 0.0 };
                let (fx, fy) = (dx as f64 - off, dy as f64 - off);
                if fx * fx + fy * fy > r2 {
                    continue;
                }
            }
            let (x, y) = (cx as i64 + dx, cy as i64 + dy);
            if x >= 0 && y >= 0 && x < width as i64 && y < height as i64 {
                out.push((x as u32, y as u32));
            }
        }
    }
    out
}

/// The tiles on a straight line from `a` to `b` (Bresenham), both ends included.
pub fn line(a: (u32, u32), b: (u32, u32)) -> Vec<(u32, u32)> {
    let (mut x, mut y) = (a.0 as i64, a.1 as i64);
    let (x1, y1) = (b.0 as i64, b.1 as i64);
    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let mut out = vec![(x as u32, y as u32)];
    while (x, y) != (x1, y1) {
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        out.push((x as u32, y as u32));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects() {
        let r = Rect::spanning((5, 9), (3, 7));
        assert_eq!(r, Rect { x: 3, y: 7, w: 3, h: 3 });
        assert_eq!(r.area(), 9);
        assert!(r.contains(5, 9) && !r.contains(6, 9) && !r.contains(2, 8) && !r.contains(4, 10));
        assert_eq!(r.tiles().count(), 9);
        assert_eq!(r.tiles().next(), Some((3, 7)));
    }

    #[test]
    fn brushes() {
        assert_eq!(brush_points(5, 5, 1, true, 10, 10), vec![(5, 5)]);
        assert_eq!(brush_points(5, 5, 2, false, 10, 10).len(), 4);
        assert_eq!(brush_points(5, 5, 3, false, 10, 10).len(), 9);
        assert_eq!(brush_points(5, 5, 3, true, 10, 10).len(), 9);
        assert_eq!(brush_points(5, 5, 5, true, 10, 10).len(), 21);
        assert_eq!(brush_points(5, 5, 4, true, 10, 10).len(), 12);
        assert_eq!(brush_points(0, 0, 3, false, 10, 10).len(), 4, "clipped at the edges");
        assert_eq!(brush_points(9, 9, 3, false, 10, 10).len(), 4);
        assert_eq!(brush_points(5, 5, 0, false, 10, 10).len(), 1);
    }

    #[test]
    fn lines() {
        assert_eq!(line((0, 0), (3, 1)), vec![(0, 0), (1, 0), (2, 1), (3, 1)]);
        assert_eq!(line((3, 3), (3, 0)).len(), 4);
        assert_eq!(line((2, 2), (2, 2)), vec![(2, 2)]);
        assert_eq!(line((4, 0), (0, 4)).len(), 5);
    }
}
