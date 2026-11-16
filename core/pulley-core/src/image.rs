//! Images: format and pixel size from a file's header, content ids, base64 transport.
//!
//! Pixels are never decoded here. An image travels inside the scene as the original file's
//! bytes (base64), identified by the SHA-256 of those bytes; After Effects imports the file
//! itself. Only formats After Effects can import are accepted, recognized by their signature.

use crate::error::{Error, ErrorCode, Result};
use crate::limits::{MAX_IMAGE_BYTES, MAX_IMAGE_SIDE};
use crate::scene::{ImageAsset, ImageFormat, Item, Scene};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt::Write as _;

/// Lowercase hex SHA-256 of `bytes`: an image's id, and the name of the file After Effects
/// imports.
pub fn content_id(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Is `id` a well-formed content id (64 lowercase hex digits)?
pub fn is_content_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Decode standard, padded base64; None if malformed or larger than `MAX_IMAGE_BYTES`.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    if text.len() / 4 * 3 > MAX_IMAGE_BYTES + 3 {
        return None;
    }
    STANDARD.decode(text).ok().filter(|b| b.len() <= MAX_IMAGE_BYTES)
}

/// The file format, from its signature, if After Effects can import it.
pub fn sniff(b: &[u8]) -> Option<ImageFormat> {
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(ImageFormat::Png)
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageFormat::Jpeg)
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if b.starts_with(b"BM") {
        Some(ImageFormat::Bmp)
    } else if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        Some(ImageFormat::Tiff)
    } else if b.starts_with(b"8BPS") {
        Some(ImageFormat::Psd)
    } else {
        None
    }
}

/// A short name for a format After Effects can't import (for warnings), from its signature.
pub fn unsupported_kind(b: &[u8]) -> &'static str {
    if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "WebP"
    } else if b.len() >= 12 && &b[4..8] == b"ftyp" {
        "HEIC/AVIF"
    } else if b.starts_with(b"<") || b.starts_with(b"\xEF\xBB\xBF<") {
        "SVG"
    } else if b.starts_with(b"%PDF") {
        "PDF"
    } else {
        "unknown"
    }
}

/// Format and pixel size of an image file After Effects can import, or None.
pub fn inspect(bytes: &[u8]) -> Option<(ImageFormat, u32, u32)> {
    let format = sniff(bytes)?;
    let size = imagesize::blob_size(bytes).ok()?;
    let w = u32::try_from(size.width).ok()?;
    let h = u32::try_from(size.height).ok()?;
    (w > 0 && h > 0 && w <= MAX_IMAGE_SIDE && h <= MAX_IMAGE_SIDE).then_some((format, w, h))
}

/// The asset for an image file's bytes, or None if After Effects can't import it.
pub fn asset(bytes: &[u8]) -> Option<ImageAsset> {
    let (format, width, height) = inspect(bytes)?;
    Some(ImageAsset { id: content_id(bytes), format, width, height, data: encode(bytes) })
}

/// One image file for After Effects to import: `name` is `<id>.<extension>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    pub name: String,
    /// The file's bytes, base64.
    pub data: String,
}

/// The files of the images a (validated) scene shows, each checked against its id: the id names
/// the file on disk, so it must really be the hash of these bytes.
pub fn media_files(scene: &Scene) -> Result<Vec<MediaFile>> {
    let mut used = HashSet::new();
    fn walk<'a>(items: &'a [Item], used: &mut HashSet<&'a str>) {
        for it in items {
            match it {
                Item::Image(im) if im.visible => {
                    used.insert(im.image.as_str());
                }
                Item::Group(g) if g.visible => walk(&g.children, used),
                _ => {}
            }
        }
    }
    walk(&scene.items, &mut used);
    let mut out = Vec::new();
    for a in scene.images.iter().filter(|a| used.contains(a.id.as_str())) {
        let bytes = decode(&a.data).ok_or_else(|| Error::data(ErrorCode::InvalidScene, "An image in the scene is not valid base64."))?;
        if content_id(&bytes) != a.id {
            return Err(Error::data(ErrorCode::InvalidScene, "An image in the scene does not match its id."));
        }
        out.push(MediaFile { name: format!("{}.{}", a.id, a.format.extension()), data: a.data.clone() });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A valid 2 x 1 PNG (red, blue), built by hand: signature, IHDR, IDAT, IEND.
    pub(crate) fn tiny_png() -> Vec<u8> {
        encode_png(2, 1, &[255, 0, 0, 0, 0, 255])
    }

    /// Minimal PNG writer for tests (8-bit RGB, no compression: stored deflate blocks).
    pub(crate) fn encode_png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
        fn crc(data: &[u8]) -> u32 {
            let mut c = 0xFFFF_FFFFu32;
            for &b in data {
                c ^= u32::from(b);
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        }
        fn adler(data: &[u8]) -> u32 {
            let (mut a, mut b) = (1u32, 0u32);
            for &x in data {
                a = (a + u32::from(x)) % 65521;
                b = (b + a) % 65521;
            }
            (b << 16) | a
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut c = kind.to_vec();
            c.extend_from_slice(data);
            out.extend_from_slice(&c);
            out.extend_from_slice(&crc(&c).to_be_bytes());
        }
        let mut raw = Vec::new();
        for row in rgb.chunks((w * 3) as usize) {
            raw.push(0);
            raw.extend_from_slice(row);
        }
        let mut z = vec![0x78, 0x01];
        for (i, block) in raw.chunks(65535).enumerate() {
            let last = (i + 1) * 65535 >= raw.len();
            z.push(u8::from(last));
            z.extend_from_slice(&(block.len() as u16).to_le_bytes());
            z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
            z.extend_from_slice(block);
        }
        z.extend_from_slice(&adler(&raw).to_be_bytes());
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &z);
        chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn formats_sizes_and_ids() {
        let png = tiny_png();
        assert_eq!(inspect(&png), Some((ImageFormat::Png, 2, 1)));
        let a = asset(&png).unwrap();
        assert!(is_content_id(&a.id));
        assert_eq!(decode(&a.data).unwrap(), png);
        // SHA-256 of "abc" (FIPS 180-2 test vector).
        assert_eq!(content_id(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let gif = b"GIF89a\x03\x00\x02\x00\x80\x00\x00".to_vec();
        assert_eq!(inspect(&gif), Some((ImageFormat::Gif, 3, 2)));
        // Not importable by After Effects: named in warnings, never accepted.
        let webp = b"RIFF\x1a\x00\x00\x00WEBPVP8 ".to_vec();
        assert_eq!(inspect(&webp), None);
        assert_eq!(unsupported_kind(&webp), "WebP");
        assert_eq!(inspect(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
        assert_eq!(inspect(&[0x89, b'P', b'N', b'G']), None, "truncated");
        assert!(!is_content_id("ABC") && !is_content_id(&"g".repeat(64)) && is_content_id(&"0".repeat(64)));
        assert_eq!(decode("not base64!"), None);
    }
}
