//! Tray icons embedded at build time from the icon registry (see build.rs).

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Transferring,
    Error,
}

/// RGBA pixels and size for a tray state. `template` selects the macOS monochrome variant.
pub fn tray_rgba(state: TrayState, template: bool) -> (Vec<u8>, u32, u32) {
    let bytes = match (state, template) {
        (TrayState::Idle, false) => TRAY_IDLE,
        (TrayState::Transferring, false) => TRAY_TRANSFERRING,
        (TrayState::Error, false) => TRAY_ERROR,
        (TrayState::Idle, true) => TRAY_IDLE_TEMPLATE,
        (TrayState::Transferring, true) => TRAY_TRANSFERRING_TEMPLATE,
        (TrayState::Error, true) => TRAY_ERROR_TEMPLATE,
    };
    decode_png(bytes).unwrap_or_else(|| fallback(state))
}

/// Decode a PNG to 8-bit RGBA.
pub fn decode_png(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if bytes.is_empty() {
        return None;
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((rgba, w, h))
}

/// A plain dot, used only if the generated icons are missing.
fn fallback(state: TrayState) -> (Vec<u8>, u32, u32) {
    let (r, g, b) = match state {
        TrayState::Idle => (120, 120, 120),
        TrayState::Transferring => (40, 120, 230),
        TrayState::Error => (220, 50, 40),
    };
    let n = 32u32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let a = if dx * dx + dy * dy <= 12.0 * 12.0 { 255 } else { 0 };
            px.extend_from_slice(&[r, g, b, a]);
        }
    }
    (px, n, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_pixels() {
        for s in [TrayState::Idle, TrayState::Transferring, TrayState::Error] {
            for t in [false, true] {
                let (px, w, h) = tray_rgba(s, t);
                assert_eq!(px.len(), (w * h * 4) as usize);
                assert!(w >= 16 && h >= 16);
            }
        }
    }
}
