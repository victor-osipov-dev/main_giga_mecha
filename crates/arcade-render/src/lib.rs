//! Converts runtime frames into Minecraft map colours.
//!
//! The MVP uses the vanilla map item as the display surface: no client mods,
//! no custom protocols, and a bounded amount of data per update.

use arcade_runtime::Frame;

/// One Minecraft map colour: base id plus a shade multiplier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapColor {
    pub id: u8,
    pub shade: u8,
}

impl MapColor {
    /// Protocol byte for a map pixel: `id * 4 + shade`.
    pub fn wire(self) -> u8 {
        self.id.wrapping_mul(4).wrapping_add(self.shade & 3)
    }
}

/// A frame converted to map pixels. Index is `y * width + x`.
#[derive(Clone, Debug)]
pub struct MapFrame {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
    pub revision: u64,
}

pub struct MapRenderer {
    width: u16,
    height: u16,
}

impl MapRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width: width.clamp(1, 128),
            height: height.clamp(1, 128),
        }
    }

    pub fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    pub fn render(&self, frame: &Frame) -> MapFrame {
        let mut pixels = Vec::with_capacity((self.width as usize) * (self.height as usize));
        for y in 0..self.height {
            let sy = (y as u32 * frame.height / self.height as u32).min(frame.height - 1);
            for x in 0..self.width {
                let sx = (x as u32 * frame.width / self.width as u32).min(frame.width - 1);
                let color = frame.get(sx, sy);
                pixels.push(quantize(color).wire());
            }
        }
        MapFrame {
            width: self.width,
            height: self.height,
            pixels,
            revision: frame.revision,
        }
    }
}

/// A compact base palette approximating the vanilla map colours. The exact
/// ids are protocol data; these are deliberately few so the first experiment
/// is cheap to reason about.
const PALETTE: [([u8; 3], u8); 12] = [
    ([0, 0, 0], 29),       // black
    ([255, 255, 255], 8),  // snow / white
    ([255, 0, 0], 28),     // red
    ([0, 255, 0], 27),     // green
    ([0, 0, 255], 25),     // blue
    ([255, 255, 0], 18),   // yellow
    ([0, 255, 255], 23),   // cyan
    ([255, 0, 255], 16),   // magenta
    ([128, 128, 128], 21), // gray
    ([64, 64, 64], 29),    // dark gray, black base with low shade
    ([255, 128, 0], 15),   // orange
    ([128, 64, 0], 26),    // brown
];

pub fn quantize(rgb: [u8; 3]) -> MapColor {
    let mut best = PALETTE[0];
    let mut best_distance = u32::MAX;
    for (color, id) in PALETTE {
        let dr = rgb[0] as i32 - color[0] as i32;
        let dg = rgb[1] as i32 - color[1] as i32;
        let db = rgb[2] as i32 - color[2] as i32;
        let distance = (dr * dr + dg * dg + db * db) as u32;
        if distance < best_distance {
            best_distance = distance;
            best = (color, id);
        }
    }
    // Four brightness steps inside the base colour's ramp.
    let lum = (rgb[0] as u32 * 30 + rgb[1] as u32 * 59 + rgb[2] as u32 * 11) / 100;
    let shade = if lum > 200 {
        3
    } else if lum > 120 {
        2
    } else if lum > 50 {
        1
    } else {
        0
    };
    let _ = best.0;
    MapColor { id: best.1, shade }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantizes_known_colours() {
        assert_eq!(quantize([0, 0, 0]).id, 29);
        assert_eq!(quantize([255, 255, 255]).id, 8);
        assert_eq!(quantize([250, 0, 0]).id, 28);
    }

    #[test]
    fn renders_scaled_frame() {
        let mut frame = Frame::solid(4, 4, [0, 0, 0]);
        frame.set(0, 0, [255, 255, 255]);
        let renderer = MapRenderer::new(128, 128);
        let map = renderer.render(&frame);
        assert_eq!(map.pixels.len(), 128 * 128);
        assert_eq!(map.pixels[0], quantize([255, 255, 255]).wire());
    }
}
