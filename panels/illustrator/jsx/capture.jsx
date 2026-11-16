/*
 * Pulley: Illustrator capture (ExtendScript). Fixed, pre-written functions only.
 *
 * Reads the current selection WITHOUT modifying the user's document: properties and path
 * points are only read. Two things need a copy (text outlines for the missing-font fallback,
 * and the artwork inside symbol instances); for those, the items are duplicated into a
 * temporary document, processed there, and the temporary document is closed without saving.
 *
 * Geometry is sent as absolute document coordinates (anchor, left and right direction per
 * point). pulley-core (Rust) converts it to AE's relative tangents, flips the y axis and
 * applies units. The only arithmetic here is applying a spot color's tint before Illustrator
 * converts the color.
 *
 * Measured on Illustrator 29.5 (see docs/ILLUSTRATOR_READ_BENCHMARK.md): reading 2,000 path
 * points takes ~31 ms, while duplicating an item into another document takes ~150 ms, so
 * copies are kept to text outlines, symbols and very large paths (reading one path's points
 * gets quadratically slower with its length: 5,000 points take ~8 s). Copies go into one
 * temporary document, each on its own layer named after the item's token, and that document
 * is exported once as SVG to a path chosen by Pulley; pulley-core reads the geometry from it.
 *
 * Effects (shadows, glows, blurs, brushes and other appearance the DOM can't describe) and
 * embedded images are handled in the same temporary document: an item whose appearance reaches
 * beyond its geometry is duplicated there and expanded (Object > Expand Appearance), which
 * turns it into plain shapes, live text and embedded pictures; each picture, and each embedded
 * image of the selection, is saved as a transparent PNG next to the SVG (Document.imageCapture,
 * at the picture's own resolution). The core places the PNGs, and the panel deletes them.
 *
 * Requires PulleyJSON (json.jsx).
 */
var PulleyAI = (function () {
    var FORMAT = "pulley-ai-capture";
    var VERSION = 2;

    function fail(code, message, hint) {
        return PulleyJSON.stringify({ ok: false, error: { code: code, message: message, hint: hint } });
    }

    function blendName(bm) {
        switch (bm) {
            case BlendModes.MULTIPLY: return "multiply";
            case BlendModes.SCREEN: return "screen";
            case BlendModes.OVERLAY: return "overlay";
            case BlendModes.SOFTLIGHT: return "softLight";
            case BlendModes.HARDLIGHT: return "hardLight";
            case BlendModes.COLORDODGE: return "colorDodge";
            case BlendModes.COLORBURN: return "colorBurn";
            case BlendModes.DARKEN: return "darken";
            case BlendModes.LIGHTEN: return "lighten";
            case BlendModes.DIFFERENCE: return "difference";
            case BlendModes.EXCLUSION: return "exclusion";
            case BlendModes.HUE: return "hue";
            case BlendModes.SATURATIONBLEND: return "saturation";
            case BlendModes.COLORBLEND: return "color";
            case BlendModes.LUMINOSITY: return "luminosity";
        }
        return "normal";
    }

    function capName(c) {
        if (c === StrokeCap.ROUNDENDCAP) { return "round"; }
        if (c === StrokeCap.PROJECTINGENDCAP) { return "square"; }
        return "butt";
    }

    function joinName(j) {
        if (j === StrokeJoin.ROUNDENDJOIN) { return "round"; }
        if (j === StrokeJoin.BEVELENDJOIN) { return "bevel"; }
        return "miter";
    }

    function justName(j) {
        switch (j) {
            case Justification.CENTER: return "center";
            case Justification.RIGHT: return "right";
            case Justification.FULLJUSTIFYLASTLINELEFT: return "justifyLeft";
            case Justification.FULLJUSTIFYLASTLINECENTER: return "justifyCenter";
            case Justification.FULLJUSTIFYLASTLINERIGHT: return "justifyRight";
            case Justification.FULLJUSTIFY: return "justifyAll";
        }
        return "left";
    }

    function arr(a) {
        var out = [];
        if (!a) { return out; }
        for (var i = 0; i < a.length; i++) { out.push(a[i]); }
        return out;
    }

    function matrixOf(m) {
        return [m.mValueA, m.mValueB, m.mValueC, m.mValueD, m.mValueTX, m.mValueTY];
    }

    function conv(space, comps) {
        // Illustrator's own conversion, using the document's color settings. 0..255 RGB.
        return arr(app.convertSampleColor(space, comps, ImageColorSpace.RGB, ColorConvertPurpose.defaultpurpose));
    }

    function spotOf(c) {
        var base = c.spot.color, t = c.tint / 100;
        switch (base.typename) {
            case "CMYKColor":
                return { rgb: conv(ImageColorSpace.CMYK, [base.cyan * t, base.magenta * t, base.yellow * t, base.black * t]) };
            case "RGBColor":
                return { rgb: [255 - (255 - base.red) * t, 255 - (255 - base.green) * t, 255 - (255 - base.blue) * t] };
            case "LabColor":
                return { rgb: conv(ImageColorSpace.LAB, [100 - (100 - base.l) * t, base.a * t, base.b * t]) };
            case "GrayColor":
                return { rgb: conv(ImageColorSpace.GrayScale, [base.gray * t]) };
        }
        return { rgb: [0, 0, 0], unsupported: "spot color" };
    }

    function gradientOf(gc) {
        var g = gc.gradient, stops = [], i;
        for (i = 0; i < g.gradientStops.length; i++) {
            var s = g.gradientStops[i];
            var col = colorOf(s.color);
            stops.push({ at: s.rampPoint, mid: s.midPoint, op: s.opacity, rgb: col && col.rgb ? col.rgb : [0, 0, 0] });
        }
        var r = { gradient: (g.type === GradientType.RADIAL ? "radial" : "linear"), stops: stops };
        try { r.origin = arr(gc.origin); r.angle = gc.angle; r.length = gc.length; } catch (e1) {}
        try { r.hiliteAngle = gc.hiliteAngle; r.hiliteLength = gc.hiliteLength; } catch (e2) {}
        try { r.matrix = matrixOf(gc.matrix); } catch (e3) {}
        return r;
    }

    function colorOf(c) {
        if (!c) { return null; }
        switch (c.typename) {
            case "RGBColor": return { rgb: [c.red, c.green, c.blue] };
            case "CMYKColor": return { rgb: conv(ImageColorSpace.CMYK, [c.cyan, c.magenta, c.yellow, c.black]) };
            case "GrayColor": return { rgb: conv(ImageColorSpace.GrayScale, [c.gray]) };
            case "LabColor": return { rgb: conv(ImageColorSpace.LAB, [c.l, c.a, c.b]) };
            case "SpotColor": return spotOf(c);
            case "GradientColor": return gradientOf(c);
            case "NoColor": return null;
            case "PatternColor": return { unsupported: "pattern" };
        }
        return { unsupported: String(c.typename) };
    }

    function paintOf(p) {
        var r = { filled: p.filled, stroked: p.stroked, evenodd: p.evenodd };
        if (p.filled) { r.fill = colorOf(p.fillColor); }
        if (p.stroked) {
            r.stroke = colorOf(p.strokeColor);
            r.width = p.strokeWidth;
            r.cap = capName(p.strokeCap);
            r.join = joinName(p.strokeJoin);
            r.miter = p.strokeMiterLimit;
            r.dashes = arr(p.strokeDashes);
            r.dashOffset = p.strokeDashOffset;
        }
        return r;
    }

    // Absolute anchor/left/right per point, as one comma-separated string per path (keeps
    // ExtendScript arrays small: large arrays are pathologically slow in ExtendScript).
    function pathOf(p) {
        var pts = p.pathPoints, n = pts.length, out = [];
        for (var i = 0; i < n; i++) {
            var q = pts[i], a = q.anchor, l = q.leftDirection, r = q.rightDirection;
            out.push(a[0], a[1], l[0], l[1], r[0], r[1]);
        }
        return { closed: p.closed, pts: out.join(",") };
    }

    function boundsCheck(it) {
        // Visible bounds much larger than the geometry plus the stroke suggest an appearance
        // the DOM can't describe (art brushes, live effects, extra strokes).
        try {
            var g = it.geometricBounds, v = it.visibleBounds;
            var sw = 0;
            try { if (it.stroked) { sw = it.strokeWidth * (it.strokeMiterLimit || 4); } } catch (e1) {}
            var slack = Math.max(sw, 1) + 0.5;
            return (g[0] - v[0] > slack) || (v[2] - g[2] > slack) || (v[1] - g[1] > slack) || (g[3] - v[3] > slack);
        } catch (e) { return false; }
    }

    function styleKey(ca, fill) {
        var f = ca.textFont;
        return [f.name, ca.size, ca.tracking, ca.autoLeading, ca.leading, ca.baselineShift, PulleyJSON.stringify(fill)].join("|");
    }

    function textOf(tf) {
        var r = { textKind: (tf.kind === TextType.AREATEXT ? "area" : (tf.kind === TextType.PATHTEXT ? "path" : "point")) };
        r.contents = tf.contents;
        try { r.anchor = arr(tf.anchor); } catch (eAn) {}
        r.matrix = matrixOf(tf.matrix);
        r.bounds = arr(tf.geometricBounds);
        if (tf.kind === TextType.AREATEXT) {
            try { var tp = tf.textPath; r.area = [tp.left, tp.top, tp.width, tp.height]; } catch (eA) {}
        }
        try { r.justification = justName(tf.paragraphs[0].paragraphAttributes.justification); } catch (eJ) { r.justification = "left"; }
        var runs = [], chars = tf.textRange.characters, n = chars.length, cur = null, offset = 0;
        for (var i = 0; i < n; i++) {
            var ch = chars[i], ca = ch.characterAttributes, len = ch.contents.length;
            var fill = ca.fillColor ? colorOf(ca.fillColor) : null;
            var key = styleKey(ca, fill);
            if (cur && cur.key === key) {
                cur.end += len;
            } else {
                var f = ca.textFont;
                cur = {
                    key: key, start: offset, end: offset + len,
                    font: f.name, family: f.family, style: f.style,
                    size: ca.size, tracking: ca.tracking, autoLeading: ca.autoLeading, leading: ca.leading,
                    baselineShift: ca.baselineShift, fill: fill
                };
                runs.push(cur);
            }
            offset += len;
        }
        for (var j = 0; j < runs.length; j++) { delete runs[j].key; }
        r.runs = runs;
        return r;
    }

    function unsupportedKind(t) {
        switch (t) {
            case "RasterItem": case "PlacedItem": return "unsupported_raster";
            case "MeshItem": return "unsupported_gradient_mesh";
            case "GraphItem": return "unsupported_graph";
            case "PluginItem": return "unsupported_plugin_item";
            case "LegacyTextItem": return "unsupported_legacy_text";
        }
        return "unsupported_item";
    }

    /**
     * Placed (linked) and raster (embedded) images: the linked file's path and size, and the
     * item's own box and matrix, from which pulley-core places the image's pixels. Embedded
     * pixels can't be read by script; only the fact is recorded.
     */
    function imageOf(it, t) {
        var r = { kind: t === "PlacedItem" ? "placed" : "raster", embedded: false };
        if (t === "RasterItem") { try { r.embedded = it.embedded !== false; } catch (eE) { r.embedded = true; } }
        if (!r.embedded) {
            try {
                var f = it.file;
                r.path = f.fsName;
                if (!f.exists) { r.missing = true; } else { r.size = f.length; }
            } catch (eF) { r.missing = true; }
        }
        try { r.box = arr(it.boundingBox); } catch (eB) {}
        try { r.matrix = matrixOf(it.matrix); } catch (eM) {}
        try { r.bounds = arr(it.geometricBounds); } catch (eG) {}
        return r;
    }

    /**
     * Does a group draw beyond all of its children, so the group itself has an effect? (The same
     * rule for clipping groups: Illustrator reports their visible bounds as their contents'.)
     */
    function groupHasEffect(g) {
        try {
            var v = g.visibleBounds, items = g.pageItems, n = items.length, u = null, i, b;
            if (n === 0) { return false; }
            for (i = 0; i < n; i++) {
                if (items[i].hidden) { continue; }
                b = items[i].visibleBounds;
                u = u ? [Math.min(u[0], b[0]), Math.max(u[1], b[1]), Math.max(u[2], b[2]), Math.min(u[3], b[3])] : [b[0], b[1], b[2], b[3]];
            }
            if (!u) { return false; }
            var slack = 0.5;
            return (u[0] - v[0] > slack) || (v[2] - u[2] > slack) || (v[1] - u[1] > slack) || (u[3] - v[3] > slack);
        } catch (e) { return false; }
    }

    /** Does a text frame draw beyond its text (an effect)? Character strokes are allowed for. */
    function textHasEffect(tf) {
        try {
            var g = tf.geometricBounds, v = tf.visibleBounds, slack = 1;
            try { var ca = tf.textRange.characterAttributes; if (ca.strokeColor && ca.strokeColor.typename !== "NoColor") { slack += ca.strokeWeight; } } catch (eS) {}
            return (g[0] - v[0] > slack) || (v[2] - g[2] > slack) || (v[1] - g[1] > slack) || (g[3] - v[3] > slack);
        } catch (e) { return false; }
    }

    /**
     * Copy `it` to the end of `layer` at the same numeric position (the temporary artboard
     * matches the source's). Copies have always landed in place (measured: setting the position
     * anyway costs ~125 ms per copy), so it is set only if it differs.
     */
    function copyInPlace(it, layer) {
        var c = it.duplicate(layer, ElementPlacement.PLACEATEND);
        try {
            var a = it.position, b = c.position;
            if (Math.abs(a[0] - b[0]) > 1e-6 || Math.abs(a[1] - b[1]) > 1e-6) { c.position = a; }
        } catch (eP) {}
        return c;
    }

    // Pictures are saved at the resolution they carry (72 / their matrix scale), within limits.
    var PICTURE_MIN_PPI = 72, PICTURE_MAX_PPI = 600, PICTURE_MAX_PIXELS = 36000000;

    /**
     * Save `it` (a raster item) as a transparent PNG at `path`: alone on a new layer of `doc`
     * (the temporary document), every other layer hidden while capturing. An item of the user's
     * document is copied there; an item already in `doc` (from an expanded copy, discarded
     * afterwards) is moved there, which is cheaper.
     */
    function capturePicture(doc, it, meta, path, own) {
        var iso = doc.layers.add(), hidden = [], i;
        try {
            var c;
            if (own) { it.move(iso, ElementPlacement.PLACEATEND); c = it; } else { c = copyInPlace(it, iso); }
            // The item's own opacity and blend mode travel separately (After Effects layer
            // properties); the picture holds the pixels alone.
            try { c.opacity = 100; c.blendingMode = BlendModes.NORMAL; } catch (eO) {}
            for (i = 0; i < doc.layers.length; i++) {
                var L = doc.layers[i];
                if (L !== iso && L.visible) { L.visible = false; hidden.push(L); }
            }
            var b = c.visibleBounds, ppi = PICTURE_MIN_PPI;
            try { var m = c.matrix; var sc = Math.sqrt(m.mValueA * m.mValueA + m.mValueB * m.mValueB); if (sc > 0) { ppi = 72 / sc; } } catch (eM) {}
            ppi = Math.max(PICTURE_MIN_PPI, Math.min(PICTURE_MAX_PPI, ppi));
            var px = (b[2] - b[0]) * (b[1] - b[3]) * (ppi / 72) * (ppi / 72);
            if (px > PICTURE_MAX_PIXELS) { ppi = ppi * Math.sqrt(PICTURE_MAX_PIXELS / px); }
            var o = new ImageCaptureOptions();
            o.resolution = ppi;
            o.antiAliasing = true;
            o.transparency = true;
            doc.imageCapture(new File(path), b, o);
            meta.image.capture = path;
            meta.image.captureBounds = arr(b);
        } catch (e) {
            meta.image.captureError = String(e);
        } finally {
            for (i = 0; i < hidden.length; i++) { try { hidden[i].visible = true; } catch (eV) {} }
            try { iso.remove(); } catch (eR) {}
        }
    }

    var BIG_PATH = 1000;

    function walk(it, state, deferred) {
        var t = it.typename;
        var m = { token: "p" + (++state.n), type: t, name: it.name };
        try { m.uuid = String(it.uuid); } catch (eU) {}
        try { m.opacity = it.opacity; } catch (eO) {}
        try { m.blend = blendName(it.blendingMode); } catch (eB) {}
        try { if (it.hidden === true) { m.hidden = true; return m; } } catch (eH) {}
        if (t === "GroupItem") {
            m.clipped = it.clipped;
            m.children = [];
            var items = it.pageItems, before = deferred.length;
            for (var k = 0; k < items.length; k++) { m.children.push(walk(items[k], state, deferred)); }
            if (state.effects && groupHasEffect(it)) {
                // The group's expanded copy replaces everything inside it: drop their own copies.
                deferred.length = before;
                m.appearance = true;
                deferred.push({ kind: "expand", item: it, meta: m });
            }
            return m;
        }
        if (t === "CompoundPathItem") {
            var paths = [], ps = it.pathItems, total = 0, j;
            for (j = 0; j < ps.length; j++) { total += ps[j].pathPoints.length; }
            if (total > BIG_PATH) {
                deferred.push({ kind: "geometry", item: it, meta: m });
            } else {
                for (j = 0; j < ps.length; j++) { paths.push(pathOf(ps[j])); }
                m.paths = paths;
            }
            if (ps.length > 0) {
                m.clipping = ps[0].clipping;
                m.paint = paintOf(ps[0]);
                m.appearance = boundsCheck(ps[0]);
                if (state.effects && m.appearance && !m.clipping) { deferred.push({ kind: "expand", item: it, meta: m }); }
            }
            return m;
        }
        if (t === "PathItem") {
            if (it.guides) { m.guide = true; return m; }
            m.clipping = it.clipping;
            m.paint = paintOf(it);
            if (it.pathPoints.length > BIG_PATH) {
                deferred.push({ kind: "geometry", item: it, meta: m });
            } else {
                m.paths = [pathOf(it)];
            }
            m.appearance = boundsCheck(it);
            if (state.effects && m.appearance && !m.clipping) { deferred.push({ kind: "expand", item: it, meta: m }); }
            return m;
        }
        if (t === "TextFrame") {
            m.text = textOf(it);
            if (state.effects && textHasEffect(it)) {
                // Expanded instead (the copy keeps live text); no outline copy needed.
                m.appearance = true;
                deferred.push({ kind: "expand", item: it, meta: m });
            } else if (state.outlines) {
                deferred.push({ kind: "outline", item: it, meta: m });
            }
            return m;
        }
        if (t === "SymbolItem") {
            try { m.symbol = it.symbol.name; } catch (eS) {}
            deferred.push({ kind: "symbol", item: it, meta: m });
            return m;
        }
        if (t === "PlacedItem" || t === "RasterItem") {
            m.image = imageOf(it, t);
            // Embedded pixels can't be read by script: save a picture of them from a copy.
            if (m.image.embedded) { deferred.push({ kind: "picture", item: it, meta: m }); }
            return m;
        }
        m.unsupported = unsupportedKind(t);
        return m;
    }

    function topLevel(sel) {
        var out = [], i;
        for (i = 0; i < sel.length; i++) {
            var it = sel[i], p = it.parent, nested = false;
            while (p && p.typename === "GroupItem") {
                for (var j = 0; j < sel.length; j++) { if (sel[j] === p) { nested = true; break; } }
                if (nested) { break; }
                p = p.parent;
            }
            if (!nested) { out.push(it); }
        }
        return out;
    }

    // Text outlines, symbol contents and very large paths: copies in one temporary document,
    // each on its own layer named after the item's token (layers export as <g id="token">).
    // The document stays active while it is modified (background documents are ~100x slower).
    /**
     * Expand a copy of `d.item` (Object > Expand Appearance) in `tmp` and read the result as the
     * item's `expanded` capture; its embedded pictures are saved as PNGs.
     */
    function tick(state, key, t0) {
        var t = state.timing || (state.timing = {});
        t[key] = (t[key] || 0) + (new Date().getTime() - t0);
        return new Date().getTime();
    }

    function expandCopy(tmp, d, state, svgPath) {
        if (!state.fxLayer) { state.fxLayer = tmp.layers.add(); state.fxLayer.name = "pulley_fx"; }
        var lay = state.fxLayer;
        var t0 = new Date().getTime();
        var dup = null;
        try {
            dup = copyInPlace(d.item, lay);
            t0 = tick(state, "duplicate", t0);
            tmp.selection = null;
            dup.selected = true;
            app.executeMenuCommand("expandStyle");
            var res = tmp.selection;
            t0 = tick(state, "expand", t0);
            // Nothing selected afterwards: nothing was expanded.
            if (!res || res.length === 0) { return false; }
            // The expanded artwork is read like a selection, without text outlines or copies.
            var sub = { n: state.n, outlines: false, effects: false };
            var pictures = [];
            var em;
            if (res.length === 1) {
                em = walk(res[0], sub, pictures);
            } else {
                // Several objects (topmost first, as selected): read them as one group.
                em = { token: "p" + (++sub.n), type: "GroupItem", name: d.meta.name, opacity: 100, blend: "normal", clipped: false, children: [] };
                for (var k = 0; k < res.length; k++) { em.children.push(walk(res[k], sub, pictures)); }
            }
            state.n = sub.n;
            t0 = tick(state, "read", t0);
            // Nothing gained (a false alarm: no picture, and text or a group expanded into itself,
            // like area text whose letters reach past its frame): keep the plain capture.
            var gotPicture = false;
            for (var g = 0; g < pictures.length; g++) { if (pictures[g].kind === "picture") { gotPicture = true; } }
            var same = em.type === "TextFrame" || (em.type === "GroupItem" && d.meta.type === "GroupItem" && (em.children || []).length === (d.meta.children || []).length);
            if (!gotPicture && same) { return false; }
            for (var i = 0; i < pictures.length; i++) {
                var p = pictures[i];
                if (p.kind === "picture") {
                    capturePicture(tmp, p.item, p.meta, svgPath + "-" + p.meta.token + ".png", true);
                } else if (p.kind === "geometry") {
                    // Very long paths are read directly here (no SVG copy for expanded artwork).
                    if (p.item.typename === "CompoundPathItem") {
                        p.meta.paths = [];
                        for (var q = 0; q < p.item.pathItems.length; q++) { p.meta.paths.push(pathOf(p.item.pathItems[q])); }
                    } else {
                        p.meta.paths = [pathOf(p.item)];
                    }
                } else {
                    p.meta.deferredError = "not readable inside an expanded effect";
                }
            }
            t0 = tick(state, "pictures", t0);
            d.meta.expanded = em;
            return true;
        } catch (e) {
            d.meta.expandError = String(e);
        } finally {
            // Clear the shared layer for the next item.
            try { while (lay.pageItems.length > 0) { lay.pageItems[0].remove(); } } catch (eR) {}
        }
    }

    function processDeferred(src, deferred, svgPath, state) {
        if (deferred.length === 0) { return false; }
        var ab = src.artboards[src.artboards.getActiveArtboardIndex()].artboardRect;
        var tmp = app.documents.add(src.documentColorSpace, Math.max(1, ab[2] - ab[0]), Math.max(1, ab[1] - ab[3]));
        try {
            tmp.artboards[0].artboardRect = ab;
            // Reference square at the source artboard's top-left corner: pulley-core uses it to
            // map SVG coordinates back to the artboard, whatever origin the exporter picks.
            var base = tmp.layers[0];
            base.name = "pulley_ref";
            var ref = base.pathItems.rectangle(ab[1], ab[0], 10, 10);
            ref.name = "pulley_ref_rect";
            ref.filled = true;
            ref.stroked = false;
            // Pictures and expanded effects first; their layers are gone before the SVG export.
            var svgJobs = [];
            for (var j = 0; j < deferred.length; j++) {
                var dj = deferred[j];
                if (dj.kind === "picture") { var tp = new Date().getTime(); capturePicture(tmp, dj.item, dj.meta, svgPath + "-" + dj.meta.token + ".png", false); tick(state, "pictures", tp); }
                else if (dj.kind === "expand") {
                    var used = expandCopy(tmp, dj, state, svgPath);
                    // Text that turned out to have no effect still gets its outline copy.
                    if (!used && !dj.meta.expandError && dj.item.typename === "TextFrame" && state.outlines) { svgJobs.push({ kind: "outline", item: dj.item, meta: dj.meta }); }
                    if (!used && !dj.meta.expandError) { dj.meta.appearance = false; }
                }
                else { svgJobs.push(dj); }
            }
            if (state.fxLayer) { try { state.fxLayer.remove(); } catch (eL) {} state.fxLayer = null; }
            if (svgJobs.length === 0) { return false; }
            deferred = svgJobs;
            for (var i = 0; i < deferred.length; i++) {
                var d = deferred[i];
                try {
                    var lay = tmp.layers.add();
                    lay.name = d.meta.token + (d.kind === "outline" ? "_outline" : "_svg");
                    // Same numeric position as the source, so it sits where the source sits
                    // relative to the reference square.
                    var dup = copyInPlace(d.item, lay);
                    if (d.kind === "outline") { dup.createOutline(); }
                    else if (d.kind === "symbol") { dup.breakLink(); }
                    if (d.kind === "outline") { d.meta.outline = lay.name; } else { d.meta.svg = lay.name; }
                } catch (eD) { d.meta.deferredError = String(eD); }
            }
            var o = new ExportOptionsSVG();
            try { o.embedRasterImages = false; } catch (e1) {}
            try { o.cssProperties = SVGCSSPropertyLocation.PRESENTATIONATTRIBUTES; } catch (e2) {}
            try { o.fontType = SVGFontType.SVGFONT; } catch (e3) {}
            try { o.fontSubsetting = SVGFontSubsetting.None; } catch (e4) {}
            try { o.documentEncoding = SVGDocumentEncoding.UTF8; } catch (e5) {}
            try { o.DTD = SVGDTDVersion.SVG1_1; } catch (e6) {}
            try { o.coordinatePrecision = 5; } catch (e7) {}
            try { o.preserveEditability = false; } catch (e8) {}
            try { o.includeFileInfo = false; } catch (e9) {}
            try { o.includeVariablesAndDatasets = false; } catch (e10) {}
            try { o.sVGAutoKerning = false; } catch (e11) {}
            try { o.sVGTextOnPath = false; } catch (e12) {}
            try { o.saveMultipleArtboards = false; } catch (e13) {}
            tmp.exportFile(new File(svgPath), ExportType.SVG, o);
            // The SVG exporter writes rasterized effects of outlined copies as <name>-<n>.png
            // files next to the SVG; Pulley reads only the geometry, so remove them.
            try {
                var svgFile = new File(svgPath), stem = decodeURI(svgFile.name).replace(/\.svg$/i, "");
                var extra = svgFile.parent.getFiles(stem + "-*.png");
                for (var x = 0; x < extra.length; x++) {
                    if (/-\d+\.png$/i.test(decodeURI(extra[x].name))) { extra[x].remove(); }
                }
            } catch (eX) {}
        } finally {
            tmp.close(SaveOptions.DONOTSAVECHANGES);
            app.activeDocument = src;
        }
        return true;
    }

    /**
     * Capture the current selection. `withOutlines` ("1"/"0") controls whether text outlines
     * are captured for the missing-font fallback. `svgPath` is a file path in Pulley's temp
     * folder, used only if copies were needed (then `capture.svg` is true). Returns JSON text:
     * { ok: true, capture: {...} } or { ok: false, error: {...} }.
     */
    function capture(withOutlines, svgPath, withEffects) {
        if (app.documents.length === 0) {
            return fail("no_document", "No document is open in Illustrator.", "Open a document, select some artwork and push again.");
        }
        var src = app.activeDocument;
        var sel = src.selection;
        if (!sel || sel.typename === "TextRange") {
            return fail("text_editing", "Illustrator is editing text.", "Press Esc to leave text editing, select the text frame and push again.");
        }
        if (sel.length === 0) {
            return fail("empty_selection", "Nothing is selected in Illustrator.", "Select the artwork to send and push again.");
        }
        var t0 = new Date().getTime();
        var tops = topLevel(sel);
        var abIndex = src.artboards.getActiveArtboardIndex();
        var cap = {
            format: FORMAT, version: VERSION, appVersion: app.version,
            documentName: src.name,
            colorMode: (src.documentColorSpace === DocumentColorSpace.CMYK ? "cmyk" : "rgb"),
            scaleFactor: 1,
            artboard: arr(src.artboards[abIndex].artboardRect),
            items: [],
            timing: {}
        };
        try { cap.scaleFactor = src.scaleFactor || 1; } catch (eSF) {}
        var savedBefore = src.saved;
        var state = { n: 0, outlines: withOutlines !== "0", effects: withEffects !== "0" };
        var deferred = [];
        try {
            for (var i = 0; i < tops.length; i++) { cap.items.push(walk(tops[i], state, deferred)); }
            cap.timing.read = new Date().getTime() - t0;
            var t1 = new Date().getTime();
            cap.svg = processDeferred(src, deferred, svgPath, state);
            cap.timing.copies = new Date().getTime() - t1;
            if (state.timing) { cap.timing.effects = state.timing; }
        } catch (e) {
            try { app.activeDocument = src; } catch (eA) {}
            return fail("capture_failed", "Reading the selection failed: " + e.toString() + (e.line ? " (line " + e.line + ")" : ""), "Try a smaller selection. If it keeps failing, save the log (Open log) and report it.");
        }
        cap.sourceSavedUnchanged = (src.saved === savedBefore);
        return PulleyJSON.stringify({ ok: true, capture: cap });
    }

    return { capture: capture };
})();

function pulleyAiCapture(withOutlines, svgPath, withEffects) {
    return PulleyAI.capture(withOutlines, svgPath, withEffects);
}
