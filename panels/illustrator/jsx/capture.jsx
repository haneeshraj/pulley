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
            var items = it.pageItems;
            for (var k = 0; k < items.length; k++) { m.children.push(walk(items[k], state, deferred)); }
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
            return m;
        }
        if (t === "TextFrame") {
            m.text = textOf(it);
            if (state.outlines) { deferred.push({ kind: "outline", item: it, meta: m }); }
            return m;
        }
        if (t === "SymbolItem") {
            try { m.symbol = it.symbol.name; } catch (eS) {}
            deferred.push({ kind: "symbol", item: it, meta: m });
            return m;
        }
        if (t === "PlacedItem" || t === "RasterItem") {
            m.image = imageOf(it, t);
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
    function processDeferred(src, deferred, svgPath) {
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
            for (var i = 0; i < deferred.length; i++) {
                var d = deferred[i];
                try {
                    var lay = tmp.layers.add();
                    lay.name = d.meta.token + (d.kind === "outline" ? "_outline" : "_svg");
                    var dup = d.item.duplicate(lay, ElementPlacement.PLACEATEND);
                    // Same numeric position as the source, so it sits where the source sits
                    // relative to the reference square.
                    dup.position = d.item.position;
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
    function capture(withOutlines, svgPath) {
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
        var state = { n: 0, outlines: withOutlines !== "0" };
        var deferred = [];
        try {
            for (var i = 0; i < tops.length; i++) { cap.items.push(walk(tops[i], state, deferred)); }
            cap.timing.read = new Date().getTime() - t0;
            var t1 = new Date().getTime();
            cap.svg = processDeferred(src, deferred, svgPath);
            cap.timing.copies = new Date().getTime() - t1;
        } catch (e) {
            try { app.activeDocument = src; } catch (eA) {}
            return fail("capture_failed", "Reading the selection failed: " + e.toString() + (e.line ? " (line " + e.line + ")" : ""), "Try a smaller selection. If it keeps failing, save the log (Open log) and report it.");
        }
        cap.sourceSavedUnchanged = (src.saved === savedBefore);
        return PulleyJSON.stringify({ ok: true, capture: cap });
    }

    return { capture: capture };
})();

function pulleyAiCapture(withOutlines, svgPath) {
    return PulleyAI.capture(withOutlines, svgPath);
}
