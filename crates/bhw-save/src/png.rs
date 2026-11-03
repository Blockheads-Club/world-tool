//! A minimal PNG encoder (8-bit RGBA, no interlacing) and the RGBA image type used for previews.

use std::io::Write;

/// An 8-bit RGBA image, rows from top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, row-major from the top-left pixel.
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// A fully transparent image.
    pub fn new(width: u32, height: u32) -> RgbaImage {
        RgbaImage { width, height, pixels: vec![0; width as usize * height as usize * 4] }
    }

    /// Sets one pixel (`y = 0` is the top row).
    pub fn put(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&rgba);
    }

    /// One pixel (`y = 0` is the top row).
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }

    /// Encodes the image as a PNG file.
    pub fn to_png(&self) -> Vec<u8> {
        let row = self.width as usize * 4;
        let mut raw = Vec::with_capacity((row + 1) * self.height as usize);
        for y in 0..self.height as usize {
            raw.push(0); // filter type: none
            raw.extend_from_slice(&self.pixels[y * row..(y + 1) * row]);
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&raw).expect("writing to a Vec cannot fail");
        let idat = z.finish().expect("writing to a Vec cannot fail");

        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&self.width.to_be_bytes());
        ihdr.extend_from_slice(&self.height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, adaptive filtering, no interlace
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &idat);
        chunk(&mut out, b"IEND", &[]);
        out
    }
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// CRC-32 (ISO 3309, as used by PNG).
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { 0xedb8_8320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_reference() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn png_round_trip_structure() {
        let mut img = RgbaImage::new(2, 1);
        img.put(1, 0, [1, 2, 3, 4]);
        assert_eq!(img.get(1, 0), [1, 2, 3, 4]);
        let png = img.to_png();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.ends_with(&[0xae, 0x42, 0x60, 0x82]));
        // IHDR: width 2, height 1
        assert_eq!(&png[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
        // inflate IDAT and compare with the filtered rows
        let len = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
        let mut d = flate2::read::ZlibDecoder::new(&png[41..41 + len]);
        let mut raw = Vec::new();
        std::io::Read::read_to_end(&mut d, &mut raw).unwrap();
        assert_eq!(raw, [0, 0, 0, 0, 0, 1, 2, 3, 4]);
    }
}
