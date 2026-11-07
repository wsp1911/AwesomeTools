//! The application icon, drawn from code so the repository stores no binaries.
//!
//! The same geometry feeds every surface that shows an icon: the tray icon at
//! runtime, and the executable, taskbar, title bar and file manager, which take
//! theirs from the ID 1 icon resource that `build.rs` compiles in.

/// Rounded rectangle in normalized coordinates, so one description covers every
/// icon size.
struct Rect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
}

impl Rect {
    const fn new(x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) -> Self {
        Self {
            x0,
            y0,
            x1,
            y1,
            radius,
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        if x < self.x0 || x > self.x1 || y < self.y0 || y > self.y1 {
            return false;
        }
        // Distance to the inner rectangle of the rounded corners.
        let dx = (self.x0 + self.radius - x)
            .max(x - (self.x1 - self.radius))
            .max(0.);
        let dy = (self.y0 + self.radius - y)
            .max(y - (self.y1 - self.radius))
            .max(0.);
        dx * dx + dy * dy <= self.radius * self.radius
    }
}

const ACCENT: [u8; 4] = [0x58, 0xa6, 0xff, 0xff];
const PAPER: [u8; 4] = [0xf4, 0xf7, 0xfb, 0xff];
const INK: [u8; 4] = [0x1b, 0x1e, 0x24, 0xff];

/// A clipboard: board, clip, sheet of paper with two lines of text. Later
/// entries are drawn on top of earlier ones.
fn shapes() -> [(Rect, [u8; 4]); 5] {
    [
        (Rect::new(0.18, 0.14, 0.82, 0.92, 0.12), ACCENT),
        (Rect::new(0.36, 0.04, 0.64, 0.26, 0.06), ACCENT),
        (Rect::new(0.28, 0.28, 0.72, 0.84, 0.06), PAPER),
        (Rect::new(0.38, 0.44, 0.62, 0.51, 0.03), INK),
        (Rect::new(0.38, 0.58, 0.55, 0.65, 0.03), INK),
    ]
}

/// Paints the icon into straight-alpha RGBA rows, top row first, which is the
/// layout both GPUI's `ImageData` and the tray icon expect.
pub fn rgba(size: u32) -> Vec<u8> {
    /// Samples per axis; coverage of each sample antialiases the edges.
    const SAMPLES: u32 = 4;

    let size = size.max(1);
    let shapes = shapes();
    let total = (SAMPLES * SAMPLES) as f32;
    let step = 1.0 / SAMPLES as f32;

    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (mut red, mut green, mut blue) = (0f32, 0f32, 0f32);
            let mut covered = 0f32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = (x as f32 + (sx as f32 + 0.5) * step) / size as f32;
                    let py = (y as f32 + (sy as f32 + 0.5) * step) / size as f32;
                    if let Some((_, color)) =
                        shapes.iter().rev().find(|(rect, _)| rect.contains(px, py))
                    {
                        red += color[0] as f32;
                        green += color[1] as f32;
                        blue += color[2] as f32;
                        covered += 1.0;
                    }
                }
            }

            let offset = ((y * size + x) * 4) as usize;
            if covered > 0.0 {
                pixels[offset] = (red / covered).round() as u8;
                pixels[offset + 1] = (green / covered).round() as u8;
                pixels[offset + 2] = (blue / covered).round() as u8;
                pixels[offset + 3] = (covered / total * 255.0).round() as u8;
            }
        }
    }
    pixels
}
