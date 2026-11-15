//! After Effects animation preset (.ffx) writer for shape-layer gradient colors.
//!
//! ExtendScript cannot set Gradient Colors (verified in AE 25.5: the property's value type is
//! NO_VALUE). AE can, however, apply an animation preset to a selected property. This module
//! writes a minimal preset holding just the "Colors" of a gradient fill/stroke; the AE plan
//! interpreter selects each gradient property and calls `applyPreset`.
//!
//! Format (RIFX, big-endian RIFF), as stored by AE itself (studied in AE 2025's own files):
//! ```text
//! RIFX FaFX
//!   head                      16-byte preset header
//!   LIST besc
//!     beso                    56-byte preset info (comp size, time base)
//!     LIST tdsp               property path the preset applies to:
//!       tdot ffffffff, tdpl <n>, n x LIST tdsi { tdix <index or ffffffff>, tdmn <match name> }
//!     tdsn Utf8 "Colors"      display name of the property the path ends at
//!     LIST tdsp               end-of-paths sentinel
//!     LIST GCst               the gradient value:
//!       LIST tdbs { tdsb 1, tdsn Utf8 "-_0_/-", tdb4 <124-byte property record>, cdat 0 }
//!       LIST GCky { Utf8 <prop.map XML with color and alpha stops> }
//! ```
//! The XML is the same `prop.map` document AE writes into projects for gradient colors.
//! Names (`tdsn`) use the current encoding: "Utf8", a 32-bit length, the text, and a zero
//! byte when needed to keep the size even. AE 2025 silently ignores a preset whose names use
//! the older zero-terminated form together with this header version. "-_0_/-" is AE's
//! placeholder for "the property's default name".

use crate::scene::GradientStop;

/// Preset header (version fields as written by After Effects 2025).
const HEAD: [u8; 16] = [0, 0, 0, 3, 0, 0, 0, 0x5f, 0, 0, 0, 1, 1, 2, 0, 0];

/// Preset info: time base, duration and a 1920x1080 reference comp, as in AE's shipped presets.
const BESO: [u8; 56] = [
    0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0x5d, 0xa8, 0, 0x1d, 0xf8, 0x52, 0, 0, 0, 4, 0, 1, 0, 1, 0x07, 0x80, 0x04, 0x38, 0x3f, 0xf0, 0, 0, 0, 0, 0, 0,
    0x3f, 0xf0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff,
];

/// Property record of a shape gradient's Colors parameter, exactly as After Effects writes it
/// (identical in AE 2025's own bundled projects).
const TDB4_COLORS_HEX: &str = "db99000100070000ffffffff000060003f1a36e2eb1c432d3ff00000000000003ff00000000000003ff00000000000003ff00000000000000001000800000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

fn tdb4_colors() -> Vec<u8> {
    (0..TDB4_COLORS_HEX.len() / 2).map(|i| u8::from_str_radix(&TDB4_COLORS_HEX[2 * i..2 * i + 2], 16).unwrap_or(0)).collect()
}

fn chunk(id: &[u8; 4], data: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

fn list(kind: &[u8; 4], body: &[u8], out: &mut Vec<u8>) {
    let mut d = Vec::with_capacity(body.len() + 4);
    d.extend_from_slice(kind);
    d.extend_from_slice(body);
    chunk(b"LIST", &d, out);
}

/// A name in the current tdsn encoding.
fn utf8_name(text: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(text.len() + 9);
    v.extend_from_slice(b"Utf8");
    v.extend_from_slice(&(text.len() as u32).to_be_bytes());
    v.extend_from_slice(text.as_bytes());
    if text.len() % 2 == 1 {
        v.push(0);
    }
    v
}

fn match_name(name: &str) -> [u8; 40] {
    let mut b = [0u8; 40];
    for (i, c) in name.bytes().take(39).enumerate() {
        b[i] = c;
    }
    b
}

fn tdsi(index: Option<u32>, name: &str, out: &mut Vec<u8>) {
    let mut body = Vec::new();
    chunk(b"tdix", &index.unwrap_or(u32::MAX).to_be_bytes(), &mut body);
    chunk(b"tdmn", &match_name(name), &mut body);
    list(b"tdsi", &body, out);
}

fn num(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{v:.8}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

/// The prop.map XML AE uses for gradient colors.
pub fn gradient_xml(stops: &[GradientStop]) -> String {
    let mut x = String::from("<?xml version='1.0'?>\n<prop.map version='4'>\n<prop.list>\n<prop.pair>\n<key>Gradient Color Data</key>\n<prop.list>\n");
    let floats = |vals: &[f64]| {
        let mut s = String::from("<array>\n<array.type><float/></array.type>\n");
        for v in vals {
            s.push_str(&format!("<float>{}</float>\n", num(*v)));
        }
        s.push_str("</array>\n");
        s
    };
    let section = |title: &str, key: &str, rows: Vec<Vec<f64>>| {
        let mut s = format!("<prop.pair>\n<key>{title}</key>\n<prop.list>\n<prop.pair>\n<key>Stops List</key>\n<prop.list>\n");
        for (i, r) in rows.iter().enumerate() {
            s.push_str(&format!(
                "<prop.pair>\n<key>Stop-{i}</key>\n<prop.list>\n<prop.pair>\n<key>{key}</key>\n{}</prop.pair>\n</prop.list>\n</prop.pair>\n",
                floats(r)
            ));
        }
        s.push_str(&format!("</prop.list>\n</prop.pair>\n<prop.pair>\n<key>Stops Size</key>\n<int type='unsigned' size='32'>{}</int>\n</prop.pair>\n</prop.list>\n</prop.pair>\n", rows.len()));
        s
    };
    let alpha: Vec<Vec<f64>> = stops.iter().map(|s| vec![s.offset, s.midpoint, s.opacity]).collect();
    let color: Vec<Vec<f64>> = stops.iter().map(|s| vec![s.offset, s.midpoint, s.color[0], s.color[1], s.color[2], 1.0]).collect();
    x.push_str(&section("Alpha Stops", "Stops Alpha", alpha));
    x.push_str(&section("Color Stops", "Stops Color", color));
    x.push_str("</prop.list>\n</prop.pair>\n<prop.pair>\n<key>Gradient Colors</key>\n<string>1.0</string>\n</prop.pair>\n</prop.list>\n</prop.map>\n");
    x
}

/// A complete .ffx file that sets a gradient's colors when applied to a selected shape
/// Gradient Fill or Gradient Stroke.
pub fn gradient_preset(stops: &[GradientStop]) -> Vec<u8> {
    let mut path = Vec::new();
    chunk(b"tdot", &u32::MAX.to_be_bytes(), &mut path);
    chunk(b"tdpl", &5u32.to_be_bytes(), &mut path);
    tdsi(None, "ADBE Root Vectors Group", &mut path);
    tdsi(Some(0), "ADBE Vector Group", &mut path);
    tdsi(None, "ADBE Vectors Group", &mut path);
    tdsi(Some(2), "ADBE Vector Graphic - G-Fill", &mut path);
    tdsi(None, "ADBE Vector Grad Colors", &mut path);

    let mut sentinel = Vec::new();
    chunk(b"tdot", &u32::MAX.to_be_bytes(), &mut sentinel);
    chunk(b"tdpl", &1u32.to_be_bytes(), &mut sentinel);
    tdsi(None, "ADBE End of path sentinel", &mut sentinel);

    let mut tdbs = Vec::new();
    chunk(b"tdsb", &1u32.to_be_bytes(), &mut tdbs);
    chunk(b"tdsn", &utf8_name("-_0_/-"), &mut tdbs);
    chunk(b"tdb4", &tdb4_colors(), &mut tdbs);
    chunk(b"cdat", &0u32.to_be_bytes(), &mut tdbs);

    let mut gcky = Vec::new();
    // No terminating zero: AE's XML parser rejects it ("Error parsing properties list from
    // memory (13)"), and AE's own files don't have one.
    chunk(b"Utf8", gradient_xml(stops).as_bytes(), &mut gcky);

    let mut gcst = Vec::new();
    list(b"tdbs", &tdbs, &mut gcst);
    list(b"GCky", &gcky, &mut gcst);

    let mut besc = Vec::new();
    chunk(b"beso", &BESO, &mut besc);
    list(b"tdsp", &path, &mut besc);
    chunk(b"tdsn", &utf8_name("Colors"), &mut besc);
    list(b"tdsp", &sentinel, &mut besc);
    list(b"GCst", &gcst, &mut besc);

    let mut body = Vec::new();
    chunk(b"head", &HEAD, &mut body);
    list(b"besc", &besc, &mut body);

    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"RIFX");
    out.extend_from_slice(&((body.len() + 4) as u32).to_be_bytes());
    out.extend_from_slice(b"FaFX");
    out.extend_from_slice(&body);
    out
}

/// Base64 (standard alphabet, padded) of [`gradient_preset`], for passing to the CEP side.
pub fn gradient_preset_base64(stops: &[GradientStop]) -> String {
    base64(&gradient_preset(stops))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops() -> Vec<GradientStop> {
        vec![
            GradientStop { offset: 0.0, color: [1.0, 0.5, 0.0], opacity: 1.0, midpoint: 0.3 },
            GradientStop { offset: 0.4, color: [0.9, 0.1, 0.5], opacity: 0.8, midpoint: 0.5 },
            GradientStop { offset: 1.0, color: [0.2, 0.0, 0.8], opacity: 1.0, midpoint: 0.5 },
        ]
    }

    /// Walk the RIFX tree and check every size field.
    fn check(buf: &[u8], off: usize, end: usize, ids: &mut Vec<String>) {
        let mut o = off;
        while o + 8 <= end {
            let id = String::from_utf8_lossy(&buf[o..o + 4]).to_string();
            let size = u32::from_be_bytes([buf[o + 4], buf[o + 5], buf[o + 6], buf[o + 7]]) as usize;
            assert!(o + 8 + size <= end, "chunk {id} overruns its parent");
            if id == "LIST" || id == "RIFX" {
                ids.push(format!("{id}:{}", String::from_utf8_lossy(&buf[o + 8..o + 12])));
                check(buf, o + 12, o + 8 + size, ids);
            } else {
                ids.push(id);
            }
            o += 8 + size + size % 2;
        }
        assert_eq!(o, end, "chunks do not fill their parent exactly");
    }

    #[test]
    fn preset_structure_is_consistent() {
        let p = gradient_preset(&stops());
        let mut ids = Vec::new();
        check(&p, 0, p.len(), &mut ids);
        let joined = ids.join(" ");
        assert!(joined.starts_with("RIFX:FaFX head LIST:besc beso LIST:tdsp tdot tdpl LIST:tdsi"), "{joined}");
        assert!(joined.contains("LIST:GCst LIST:tdbs tdsb tdsn tdb4 cdat LIST:GCky Utf8"), "{joined}");
    }

    #[test]
    fn xml_holds_every_stop() {
        let x = gradient_xml(&stops());
        assert_eq!(x.matches("<key>Stops Color</key>").count(), 3);
        assert_eq!(x.matches("<key>Stops Alpha</key>").count(), 3);
        assert!(x.contains("<float>0.4</float>\n<float>0.5</float>\n<float>0.9</float>\n<float>0.1</float>\n<float>0.5</float>\n<float>1</float>"));
        assert!(x.contains("<float>0.4</float>\n<float>0.5</float>\n<float>0.8</float>"));
        assert!(x.contains("<int type='unsigned' size='32'>3</int>"));
    }

    #[test]
    fn records_match_after_effects() {
        // The Colors property record AE writes (also found in AE 2025's bundled projects).
        let t = tdb4_colors();
        assert_eq!(t.len(), 124);
        assert_eq!(&t[..4], &[0xdb, 0x99, 0, 1]);
        assert_eq!(t[79], 1);
        assert_eq!(&BESO[28..32], &[0x07, 0x80, 0x04, 0x38]);
        // Name encoding as in AE 2025's shipped presets (and odd-length padding).
        assert_eq!(utf8_name("Trim Paths 1"), b"Utf8\0\0\0\x0cTrim Paths 1".to_vec());
        assert_eq!(utf8_name("Gradient Fill 1"), b"Utf8\0\0\0\x0fGradient Fill 1\0".to_vec());
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
