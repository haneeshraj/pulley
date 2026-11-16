//! Hard limits applied to all untrusted input before any other work.
//! Documented in schema/SCENE.md and SECURITY.md; keep the three in sync.

/// Largest scene or capture accepted, in bytes of JSON text.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
/// Deepest group nesting accepted.
pub const MAX_DEPTH: usize = 48;
/// Items in a scene, counting every level (groups, shapes, texts, outline shapes).
pub const MAX_ITEMS: usize = 100_000;
/// Paths in one shape or clip.
pub const MAX_PATHS_PER_SHAPE: usize = 10_000;
/// Vertices in one path.
pub const MAX_VERTICES_PER_PATH: usize = 100_000;
/// Vertices in the whole scene.
pub const MAX_VERTICES_TOTAL: usize = 2_000_000;
/// Largest absolute coordinate, in pixels. AE comps are at most 30,000 px.
pub const MAX_COORD: f64 = 10_000_000.0;
/// Longest item name, in UTF-16 code units.
pub const MAX_NAME_LEN: usize = 1024;
/// Longest id, in bytes.
pub const MAX_ID_LEN: usize = 256;
/// Longest text content, in UTF-16 code units.
pub const MAX_TEXT_LEN: usize = 65_536;
/// Style runs in one text item.
pub const MAX_RUNS: usize = 65_536;
/// Gradient stops in one gradient.
pub const MAX_STOPS: usize = 256;
/// Entries in a dash array.
pub const MAX_DASHES: usize = 16;
/// Fills or strokes on one shape.
pub const MAX_PAINTS: usize = 32;
/// Warnings in a scene.
pub const MAX_WARNINGS: usize = 10_000;
/// Longest warning message, in UTF-16 code units.
pub const MAX_MESSAGE_LEN: usize = 2048;
/// Longest SVG path data string, in bytes.
pub const MAX_PATH_DATA_LEN: usize = 16 * 1024 * 1024;
/// Largest SVG document accepted from Illustrator, in bytes.
pub const MAX_SVG_BYTES: usize = 64 * 1024 * 1024;
/// Largest stroke width, dash or font size-like length, in pixels.
pub const MAX_LENGTH: f64 = 100_000.0;
/// Effects (shadows, blurs) on one item.
pub const MAX_EFFECTS: usize = 16;
