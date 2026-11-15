//! Pulley build tasks.
//!
//! `cargo xtask icons`        render, pack and validate every icon in assets/icons/icons.json
//! `cargo xtask icons --check` validate masters and existing outputs without writing
//!
//! Every problem is reported with the slot id, and the command fails if there is any.
#![forbid(unsafe_code)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

type Problems = Vec<String>;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("icons") => icons(args.iter().any(|a| a == "--check")),
        _ => {
            eprintln!("usage: cargo xtask icons [--check]");
            2
        }
    };
    std::process::exit(code);
}

struct Ctx {
    dir: PathBuf,
    opts: resvg::usvg::Options<'static>,
    check_only: bool,
}

fn icons(check_only: bool) -> i32 {
    let dir = root().join("assets").join("icons");
    let reg_path = dir.join("icons.json");
    let reg: Value = match fs::read_to_string(&reg_path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("icons.json: {e}");
            return 1;
        }
    };
    let mut opts = resvg::usvg::Options::default();
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    opts.fontdb = Arc::new(db);
    let ctx = Ctx { dir: dir.clone(), opts, check_only };
    let Some(slots) = reg.get("slots").and_then(Value::as_object) else {
        eprintln!("icons.json: no \"slots\" object");
        return 1;
    };
    let mut problems: Problems = Vec::new();
    let mut outputs = 0;
    for (id, slot) in slots {
        let before = problems.len();
        match process_slot(&ctx, slots, id, slot, &mut problems) {
            Ok(n) => outputs += n,
            Err(e) => problems.push(format!("{id}: {e}")),
        }
        if problems.len() == before {
            println!("ok   {id}");
        } else {
            println!("FAIL {id}");
        }
    }
    if let Some(b) = reg.get("bundles").and_then(|b| b.get("uiJs"))
        && let Err(e) = ui_bundle(&ctx, slots, b)
    {
        problems.push(format!("bundle uiJs: {e}"));
    }
    if problems.is_empty() {
        println!("{} slots, {outputs} outputs: all valid.", slots.len());
        0
    } else {
        eprintln!("\n{} problem(s):", problems.len());
        for p in &problems {
            eprintln!("  - {p}");
        }
        1
    }
}

fn master_of(slots: &serde_json::Map<String, Value>, id: &str, slot: &Value) -> Result<String, String> {
    if let Some(m) = slot.get("master").and_then(Value::as_str) {
        return Ok(m.to_string());
    }
    if let Some(a) = slot.get("alias").and_then(Value::as_str) {
        let target = slots.get(a).ok_or_else(|| format!("alias to unknown slot \"{a}\""))?;
        return target.get("master").and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("alias \"{a}\" has no master"));
    }
    Err(format!("slot {id} has neither \"master\" nor \"alias\""))
}

fn process_slot(ctx: &Ctx, slots: &serde_json::Map<String, Value>, id: &str, slot: &Value, problems: &mut Problems) -> Result<usize, String> {
    let master_rel = master_of(slots, id, slot)?;
    let master = ctx.dir.join(&master_rel);
    let data = fs::read(&master).map_err(|e| format!("master {master_rel} cannot be read ({e})"))?;
    let kind = slot.get("kind").and_then(Value::as_str).unwrap_or("raster");
    let color = slot.get("color").and_then(Value::as_str).unwrap_or("full");
    let is_svg = master_rel.ends_with(".svg");
    if is_svg {
        let text = String::from_utf8_lossy(&data);
        for p in svg_safety(&text) {
            problems.push(format!("{id}: master {master_rel}: {p}"));
        }
        if color == "currentColor" {
            for p in current_color_only(&text) {
                problems.push(format!("{id}: master {master_rel}: {p}"));
            }
        }
    }
    let outs = slot.get("outputs").and_then(Value::as_array).ok_or("no outputs")?;
    let mut n = 0;
    for out in outs {
        let rel = out.get("path").and_then(Value::as_str).ok_or("output without path")?;
        let path = ctx.dir.join(rel);
        let format = out.get("format").and_then(Value::as_str).unwrap_or("png");
        if !ctx.check_only {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let r = match format {
                "svg" => write_svg(&data, out, &path, kind),
                "png" => write_png(ctx, &data, is_svg, out, &path),
                "ico" => write_ico(ctx, &data, is_svg, out, &path),
                "icns" => write_icns(ctx, &data, is_svg, out, &path),
                "bmp" => write_bmp(ctx, &data, is_svg, out, &path),
                other => Err(format!("unknown format {other}")),
            };
            if let Err(e) = r {
                problems.push(format!("{id}: {rel}: {e}"));
                continue;
            }
        }
        for p in validate_output(&path, format, out, slot) {
            problems.push(format!("{id}: {rel}: {p}"));
        }
        n += 1;
    }
    Ok(n)
}

/// Things that must never be in an icon master: scripts, external references, embedded HTML.
fn svg_safety(svg: &str) -> Vec<String> {
    let lower = svg.to_ascii_lowercase();
    let mut p = Vec::new();
    for bad in ["<script", "<foreignobject", "<image", "<iframe", "javascript:", "<style"] {
        if lower.contains(bad) {
            p.push(format!("contains {bad}"));
        }
    }
    for attr in [" onload=", " onclick=", " onerror=", " onmouseover="] {
        if lower.contains(attr) {
            p.push(format!("contains an event handler ({})", attr.trim()));
        }
    }
    if lower.contains("href=\"http") || lower.contains("href='http") || lower.contains("href=\"//") {
        p.push("references an external resource".into());
    }
    p
}

/// UI icons must use only currentColor so they follow the host theme.
fn current_color_only(svg: &str) -> Vec<String> {
    let mut p = Vec::new();
    for attr in ["fill=\"", "stroke=\"", "fill='", "stroke='"] {
        let mut rest = svg;
        while let Some(i) = rest.find(attr) {
            let after = &rest[i + attr.len()..];
            let quote = if attr.ends_with('"') { '"' } else { '\'' };
            let val = after.split(quote).next().unwrap_or("");
            if val != "currentColor" && val != "none" {
                p.push(format!("uses the color {val:?} (only currentColor or none is allowed)"));
            }
            rest = after;
        }
    }
    if svg.contains("style=") {
        p.push("uses a style attribute (use fill/stroke attributes with currentColor)".into());
    }
    p
}

fn render(ctx: &Ctx, data: &[u8], is_svg: bool, w: u32, h: u32) -> Result<resvg::tiny_skia::Pixmap, String> {
    use resvg::tiny_skia::{Pixmap, Transform};
    if !is_svg {
        let src = Pixmap::decode_png(data).map_err(|e| format!("master PNG cannot be decoded ({e})"))?;
        if src.width() < w || src.height() < h {
            return Err(format!("master PNG is {}x{}, smaller than the {w}x{h} output", src.width(), src.height()));
        }
        let mut px = Pixmap::new(w, h).ok_or("bad size")?;
        let paint = resvg::tiny_skia::PixmapPaint { quality: resvg::tiny_skia::FilterQuality::Bicubic, ..Default::default() };
        px.draw_pixmap(0, 0, src.as_ref(), &paint, Transform::from_scale(w as f32 / src.width() as f32, h as f32 / src.height() as f32), None);
        return Ok(px);
    }
    let tree = resvg::usvg::Tree::from_data(data, &ctx.opts).map_err(|e| format!("master SVG is invalid ({e})"))?;
    let size = tree.size();
    let mut px = Pixmap::new(w, h).ok_or("bad size")?;
    resvg::render(&tree, Transform::from_scale(w as f32 / size.width(), h as f32 / size.height()), &mut px.as_mut());
    Ok(px)
}

/// Straight (non-premultiplied) RGBA.
fn rgba(px: &resvg::tiny_skia::Pixmap) -> Vec<u8> {
    px.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

fn dims(out: &Value) -> Result<(u32, u32), String> {
    let w = out.get("width").and_then(Value::as_u64).ok_or("no width")? as u32;
    let h = out.get("height").and_then(Value::as_u64).ok_or("no height")? as u32;
    Ok((w, h))
}

fn write_png(ctx: &Ctx, data: &[u8], is_svg: bool, out: &Value, path: &Path) -> Result<(), String> {
    let (w, h) = dims(out)?;
    let px = render(ctx, data, is_svg, w, h)?;
    px.save_png(path).map_err(|e| e.to_string())
}

fn sizes(out: &Value) -> Vec<u32> {
    out.get("sizes").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as u32).collect()).unwrap_or_default()
}

fn write_ico(ctx: &Ctx, data: &[u8], is_svg: bool, out: &Value, path: &Path) -> Result<(), String> {
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for s in sizes(out) {
        let px = render(ctx, data, is_svg, s, s)?;
        let img = ico::IconImage::from_rgba_data(s, s, rgba(&px));
        // PNG-compress the large entry only (Microsoft's recommendation).
        let entry = if s >= 256 { ico::IconDirEntry::encode_as_png(&img) } else { ico::IconDirEntry::encode_as_bmp(&img) };
        dir.add_entry(entry.map_err(|e| e.to_string())?);
    }
    let f = fs::File::create(path).map_err(|e| e.to_string())?;
    dir.write(f).map_err(|e| e.to_string())
}

fn write_icns(ctx: &Ctx, data: &[u8], is_svg: bool, out: &Value, path: &Path) -> Result<(), String> {
    let mut family = icns::IconFamily::new();
    for s in sizes(out) {
        let px = render(ctx, data, is_svg, s, s)?;
        let img = icns::Image::from_data(icns::PixelFormat::RGBA, s, s, rgba(&px)).map_err(|e| e.to_string())?;
        family.add_icon(&img).map_err(|e| format!("{s}px: {e}"))?;
    }
    let f = fs::File::create(path).map_err(|e| e.to_string())?;
    family.write(f).map_err(|e| e.to_string())
}

/// 24-bit BMP (installers want opaque bitmaps): composited over white.
fn write_bmp(ctx: &Ctx, data: &[u8], is_svg: bool, out: &Value, path: &Path) -> Result<(), String> {
    let (w, h) = dims(out)?;
    let px = render(ctx, data, is_svg, w, h)?;
    let src = rgba(&px);
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut b: Vec<u8> = Vec::with_capacity(size as usize);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&size.to_le_bytes());
    b.extend_from_slice(&[0, 0, 0, 0]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&(h as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&(row * h).to_le_bytes());
    b.extend_from_slice(&2835u32.to_le_bytes());
    b.extend_from_slice(&2835u32.to_le_bytes());
    b.extend_from_slice(&[0; 8]);
    for y in (0..h).rev() {
        let mut written = 0;
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let a = u32::from(src[i + 3]);
            let blend = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
            b.extend_from_slice(&[blend(src[i + 2]), blend(src[i + 1]), blend(src[i])]);
            written += 3;
        }
        while written < row {
            b.push(0);
            written += 1;
        }
    }
    fs::write(path, b).map_err(|e| e.to_string())
}

/// Normalized SVG output: title and comments removed; for vector logos, currentColor
/// replaced with the output's fill color.
fn write_svg(data: &[u8], out: &Value, path: &Path, _kind: &str) -> Result<(), String> {
    let mut s = String::from_utf8(data.to_vec()).map_err(|_| "master is not UTF-8")?;
    s = strip_between(&s, "<title>", "</title>");
    s = strip_between(&s, "<!--", "-->");
    if let Some(fill) = out.get("fill").and_then(Value::as_str) {
        s = s.replace("currentColor", fill);
    }
    let s = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("");
    fs::write(path, s + "\n").map_err(|e| e.to_string())
}

fn strip_between(s: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(close) {
            Some(j) => rest = &rest[i + j + close.len()..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn validate_output(path: &Path, format: &str, out: &Value, slot: &Value) -> Vec<String> {
    let mut p = Vec::new();
    let Ok(bytes) = fs::read(path) else {
        p.push("missing (run `cargo xtask icons`)".into());
        return p;
    };
    let template = slot.get("color").and_then(Value::as_str) == Some("template");
    let want_alpha = slot.get("alpha").and_then(Value::as_bool).unwrap_or(true);
    match format {
        "png" => {
            let dec = png::Decoder::new(std::io::Cursor::new(&bytes));
            match dec.read_info() {
                Ok(mut r) => {
                    let info = r.info().clone();
                    if let Ok((w, h)) = dims(out)
                        && (info.width != w || info.height != h)
                    {
                        p.push(format!("is {}x{}, expected {w}x{h}", info.width, info.height));
                    }
                    let has_alpha = matches!(info.color_type, png::ColorType::Rgba | png::ColorType::GrayscaleAlpha);
                    if want_alpha && !has_alpha {
                        p.push("has no alpha channel".into());
                    }
                    if template {
                        let Some(n) = r.output_buffer_size() else { return p };
                        let mut buf = vec![0; n];
                        if let Ok(fi) = r.next_frame(&mut buf)
                            && fi.color_type == png::ColorType::Rgba
                        {
                            let bad = buf[..fi.buffer_size()].as_chunks::<4>().0.iter().filter(|c| c[3] > 0 && (c[0] > 8 || c[1] > 8 || c[2] > 8)).count();
                            if bad > 0 {
                                p.push(format!("template icon must be black + alpha only ({bad} colored pixels)"));
                            }
                        }
                    }
                }
                Err(e) => p.push(format!("is not a valid PNG ({e})")),
            }
        }
        "ico" => match ico::IconDir::read(std::io::Cursor::new(&bytes)) {
            Ok(dir) => {
                let have: Vec<u32> = dir.entries().iter().map(|e| e.width()).collect();
                for s in sizes(out) {
                    if !have.contains(&s) {
                        p.push(format!("lacks the {s}px entry"));
                    }
                }
            }
            Err(e) => p.push(format!("is not a valid .ico ({e})")),
        },
        "icns" => match icns::IconFamily::read(std::io::Cursor::new(&bytes)) {
            Ok(f) => {
                if f.available_icons().is_empty() {
                    p.push("contains no icons".into());
                }
            }
            Err(e) => p.push(format!("is not a valid .icns ({e})")),
        },
        "bmp" => {
            if bytes.len() < 54 || &bytes[0..2] != b"BM" {
                p.push("is not a BMP".into());
            } else if let Ok((w, h)) = dims(out) {
                let bw = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
                let bh = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]);
                let bpp = u16::from_le_bytes([bytes[28], bytes[29]]);
                if bw != w as i32 || bh != h as i32 {
                    p.push(format!("is {bw}x{bh}, expected {w}x{h}"));
                }
                if bpp != 24 {
                    p.push(format!("is {bpp}-bit, expected 24-bit"));
                }
            }
        }
        "svg" => {
            let s = String::from_utf8_lossy(&bytes);
            p.extend(svg_safety(&s));
            if !s.trim_start().starts_with("<svg") {
                p.push("does not start with <svg".into());
            }
        }
        _ => {}
    }
    p
}

/// panels/shared/js/icons.js: every ui.* icon as an inline SVG string.
fn ui_bundle(ctx: &Ctx, slots: &serde_json::Map<String, Value>, b: &Value) -> Result<(), String> {
    let rel = b.get("path").and_then(Value::as_str).ok_or("no path")?;
    let prefix = b.get("slots").and_then(Value::as_str).unwrap_or("ui.");
    let mut entries = Vec::new();
    for (id, slot) in slots {
        if !id.starts_with(prefix) {
            continue;
        }
        let out = slot
            .get("outputs")
            .and_then(Value::as_array)
            .and_then(|o| o.first())
            .and_then(|o| o.get("path"))
            .and_then(Value::as_str)
            .ok_or("ui slot without output")?;
        let svg = fs::read_to_string(ctx.dir.join(out)).map_err(|e| format!("{id}: {e}"))?;
        let svg = svg.trim().replacen("<svg ", "<svg width=\"16\" height=\"16\" focusable=\"false\" ", 1);
        entries.push(format!("  {}: {}", serde_json::to_string(id).unwrap_or_default(), serde_json::to_string(&svg).unwrap_or_default()));
    }
    let js = format!(
        "// Generated by `cargo xtask icons` from assets/icons/icons.json. Do not edit.\n(typeof window !== 'undefined' ? window : globalThis).PULLEY_ICONS = {{\n{}\n}};\n",
        entries.join(",\n")
    );
    let path = ctx.dir.join(rel);
    if ctx.check_only {
        let current = fs::read_to_string(&path).unwrap_or_default();
        if current != js {
            return Err(format!("{rel} is out of date (run `cargo xtask icons`)"));
        }
        return Ok(());
    }
    fs::write(path, js).map_err(|e| e.to_string())
}
