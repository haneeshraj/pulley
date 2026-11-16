/*
 * Builds Pulley's golden Illustrator fixtures (.ai) from scratch, so they are reproducible
 * and contain nothing copyrighted. Run through the test harness (tests/harness), which then
 * captures each fixture with the real capture code (capture-fixtures.jsx).
 *
 * Coordinates below are artboard-relative, y-down, in points (like the scene format).
 */

function pulleyMakeFixtures(outDir, only) {
    var made = [];
    var log = [];

    function rgb(r, g, b) { var c = new RGBColor(); c.red = r; c.green = g; c.blue = b; return c; }
    function cmyk(c, m, y, k) { var x = new CMYKColor(); x.cyan = c; x.magenta = m; x.yellow = y; x.black = k; return x; }
    function newDoc(space, w, h) {
        var d = app.documents.add(space, w, h);
        return d;
    }
    function ab(doc) { return doc.artboards[0].artboardRect; }
    function X(doc, x) { return ab(doc)[0] + x; }
    function Y(doc, y) { return ab(doc)[1] - y; }
    function rect(doc, x, y, w, h) { return doc.pathItems.rectangle(Y(doc, y), X(doc, x), w, h); }
    function ell(doc, x, y, w, h) { return doc.pathItems.ellipse(Y(doc, y), X(doc, x), w, h); }
    function fillOnly(it, c) { it.filled = true; it.fillColor = c; it.stroked = false; return it; }
    function strokeOnly(it, c, w) { it.filled = false; it.stroked = true; it.strokeColor = c; it.strokeWidth = w; return it; }
    function poly(doc, pts, closed) {
        var p = doc.pathItems.add();
        var abs = [];
        for (var i = 0; i < pts.length; i++) { abs.push([X(doc, pts[i][0]), Y(doc, pts[i][1])]); }
        p.setEntirePath(abs.slice(0, Math.min(abs.length, 1000)));
        for (var j = 1000; j < abs.length; j++) { var pp = p.pathPoints.add(); pp.anchor = abs[j]; pp.leftDirection = abs[j]; pp.rightDirection = abs[j]; }
        p.closed = closed;
        return p;
    }
    function save(doc, name) {
        var f = new File(outDir + "/" + name + ".ai");
        var o = new IllustratorSaveOptions();
        doc.saveAs(f, o);
        doc.close(SaveOptions.DONOTSAVECHANGES);
        made.push(name);
    }
    function group(doc, name, items) {
        var g = doc.groupItems.add();
        g.name = name;
        for (var i = items.length - 1; i >= 0; i--) { items[i].move(g, ElementPlacement.PLACEATBEGINNING); }
        return g;
    }
    function step(name, fn) {
        // `only`: comma-separated step names to (re)build; all when empty.
        if (only && ("," + only + ",").indexOf("," + name + ",") < 0) { return; }
        try { fn(); } catch (e) { log.push(name + ": " + e.toString() + (e.line ? " (line " + e.line + ")" : "")); while (app.documents.length > 0) { app.activeDocument.close(SaveOptions.DONOTSAVECHANGES); } }
    }

    // 1. Basic shapes
    step("shapes", function () {
        var d = newDoc(DocumentColorSpace.RGB, 800, 600);
        fillOnly(rect(d, 50, 50, 200, 120), rgb(220, 40, 30)).name = "Red Rect";
        var e = ell(d, 300, 50, 150, 100); e.name = "Blue Ellipse"; e.filled = true; e.fillColor = rgb(30, 90, 220); e.stroked = true; e.strokeColor = rgb(0, 0, 0); e.strokeWidth = 4;
        fillOnly(d.pathItems.roundedRectangle(Y(d, 250), X(d, 50), 200, 100, 20, 20), rgb(40, 160, 70)).name = "Rounded";
        fillOnly(d.pathItems.polygon(X(d, 450), Y(d, 350), 60, 6), rgb(250, 150, 20)).name = "Hexagon";
        var s = d.pathItems.star(X(d, 650), Y(d, 350), 70, 30, 5); s.name = "Star"; s.fillColor = rgb(250, 220, 40); s.strokeColor = rgb(90, 60, 0); s.strokeWidth = 2;
        strokeOnly(poly(d, [[50, 450], [300, 550]], false), rgb(200, 0, 0), 6).name = "Line";
        var c = d.pathItems.add();
        c.name = "Curve";
        var p1 = c.pathPoints.add(); p1.anchor = [X(d, 400), Y(d, 500)]; p1.leftDirection = p1.anchor; p1.rightDirection = [X(d, 450), Y(d, 420)];
        var p2 = c.pathPoints.add(); p2.anchor = [X(d, 700), Y(d, 520)]; p2.leftDirection = [X(d, 640), Y(d, 600)]; p2.rightDirection = p2.anchor;
        strokeOnly(c, rgb(20, 20, 20), 3);
        save(d, "shapes");
    });

    // 2. Nested groups with names, opacity and blend modes
    step("groups", function () {
        var d = newDoc(DocumentColorSpace.RGB, 600, 400);
        var a1 = fillOnly(rect(d, 20, 20, 100, 60), rgb(200, 50, 50)); a1.name = "A1";
        var a2 = fillOnly(rect(d, 140, 20, 100, 60), rgb(50, 200, 50)); a2.name = "A2";
        var ga = group(d, "Inner A", [a1, a2]);
        var b1 = fillOnly(ell(d, 20, 120, 120, 120), rgb(50, 50, 200)); b1.name = "B1";
        var st = fillOnly(d.pathItems.star(X(d, 240), Y(d, 180), 60, 25, 6), rgb(230, 200, 0)); st.name = "Deep Star";
        var deep = group(d, "Deep", [st]);
        var gb = group(d, "Inner B", [b1, deep]); gb.opacity = 50; gb.blendingMode = BlendModes.MULTIPLY;
        var top = fillOnly(rect(d, 400, 300, 150, 80), rgb(120, 120, 120)); top.name = "Top Rect"; top.opacity = 75; top.blendingMode = BlendModes.SCREEN;
        group(d, "Outer", [top, ga, gb]);
        save(d, "groups");
    });

    // 3. Compound paths, both fill rules
    step("compound", function () {
        var d = newDoc(DocumentColorSpace.RGB, 600, 300);
        function donut(name, x, evenodd, reverseInner) {
            var cp = d.compoundPathItems.add();
            cp.name = name;
            var outer = cp.pathItems.ellipse(Y(d, 50), X(d, x), 200, 200);
            var inner = cp.pathItems.ellipse(Y(d, 100), X(d, x + 50), 100, 100, reverseInner);
            outer.evenodd = evenodd; inner.evenodd = evenodd;
            outer.filled = true; outer.fillColor = rgb(160, 30, 160); outer.stroked = false;
            inner.filled = true; inner.fillColor = rgb(160, 30, 160); inner.stroked = false;
            return cp;
        }
        donut("Donut evenodd", 20, true, false);
        donut("Donut nonzero reversed", 220, false, true);
        donut("Donut nonzero same direction", 400, false, false);
        save(d, "compound");
    });

    // 4. Every stroke option
    step("strokes", function () {
        var d = newDoc(DocumentColorSpace.RGB, 800, 600);
        var caps = [StrokeCap.BUTTENDCAP, StrokeCap.ROUNDENDCAP, StrokeCap.PROJECTINGENDCAP];
        var capNames = ["Cap butt", "Cap round", "Cap square"];
        for (var i = 0; i < 3; i++) {
            var l = strokeOnly(poly(d, [[50, 50 + i * 40], [300, 50 + i * 40]], false), rgb(0, 0, 0), 16);
            l.strokeCap = caps[i]; l.name = capNames[i];
        }
        var joins = [StrokeJoin.MITERENDJOIN, StrokeJoin.ROUNDENDJOIN, StrokeJoin.BEVELENDJOIN];
        var joinNames = ["Join miter", "Join round", "Join bevel"];
        for (var j = 0; j < 3; j++) {
            var z = strokeOnly(poly(d, [[400 + j * 120, 200], [440 + j * 120, 60], [480 + j * 120, 200]], false), rgb(30, 60, 200), 14);
            z.strokeJoin = joins[j]; z.name = joinNames[j];
        }
        var m2 = strokeOnly(poly(d, [[50, 300], [100, 200], [150, 300]], false), rgb(200, 30, 30), 10); m2.strokeMiterLimit = 2; m2.name = "Miter 2";
        var m10 = strokeOnly(poly(d, [[200, 300], [250, 200], [300, 300]], false), rgb(200, 30, 30), 10); m10.strokeMiterLimit = 10; m10.name = "Miter 10";
        var d1 = strokeOnly(poly(d, [[50, 380], [750, 380]], false), rgb(0, 120, 0), 4); d1.strokeDashes = [10, 5]; d1.name = "Dash 10 5";
        var d2 = strokeOnly(poly(d, [[50, 420], [750, 420]], false), rgb(0, 120, 0), 4); d2.strokeDashes = [20, 10, 5, 10]; d2.strokeDashOffset = 7; d2.name = "Dash 20 10 5 10 offset 7";
        var d3 = strokeOnly(poly(d, [[50, 460], [750, 460]], false), rgb(0, 120, 0), 4); d3.strokeDashes = [1, 2, 3, 4, 5, 6]; d3.name = "Dash 3 pairs";
        var thin = strokeOnly(rect(d, 50, 500, 200, 60), rgb(0, 0, 0), 0.5); thin.name = "Hairline";
        var both = rect(d, 300, 500, 200, 60); both.filled = true; both.fillColor = rgb(255, 230, 200); both.stroked = true; both.strokeColor = rgb(120, 60, 0); both.strokeWidth = 12; both.name = "Fill and thick stroke";
        save(d, "strokes");
    });

    // 5. Linear and radial gradients
    step("gradients", function () {
        var d = newDoc(DocumentColorSpace.RGB, 800, 600);
        var lin = d.gradients.add(); lin.name = "Pulley Linear"; lin.type = GradientType.LINEAR;
        lin.gradientStops[0].color = rgb(255, 120, 0); lin.gradientStops[0].rampPoint = 0;
        lin.gradientStops[1].color = rgb(40, 0, 160); lin.gradientStops[1].rampPoint = 100; lin.gradientStops[1].opacity = 60;
        var mid = lin.gradientStops.add(); mid.color = rgb(230, 20, 120); mid.rampPoint = 40; mid.midPoint = 30;
        var g1 = new GradientColor(); g1.gradient = lin;
        var r1 = rect(d, 50, 50, 300, 150); r1.name = "Linear 0deg"; r1.filled = true; r1.fillColor = g1; r1.stroked = false;
        var r2 = rect(d, 400, 50, 300, 150); r2.name = "Linear rotated 45"; r2.filled = true; r2.fillColor = g1; r2.stroked = false;
        r2.rotate(45, false, false, true, false, Transformation.CENTER);
        var rad = d.gradients.add(); rad.name = "Pulley Radial"; rad.type = GradientType.RADIAL;
        rad.gradientStops[0].color = rgb(255, 255, 255); rad.gradientStops[0].midPoint = 70;
        rad.gradientStops[1].color = rgb(0, 80, 160);
        var g2 = new GradientColor(); g2.gradient = rad;
        var e = ell(d, 50, 250, 250, 250); e.name = "Radial"; e.filled = true; e.fillColor = g2; e.stroked = false;
        var e2 = ell(d, 350, 250, 300, 200); e2.name = "Radial scaled"; e2.filled = true; e2.fillColor = g2; e2.stroked = false;
        e2.resize(100, 60, false, false, true, false, 100, Transformation.CENTER);
        var gs = rect(d, 700, 300, 60, 250); gs.name = "Gradient stroke"; gs.filled = false; gs.stroked = true; gs.strokeWidth = 12; gs.strokeColor = g1;
        save(d, "gradients");
    });

    // 6. Clipping masks
    step("clipping", function () {
        var d = newDoc(DocumentColorSpace.RGB, 600, 400);
        var a = fillOnly(ell(d, 0, 0, 250, 250), rgb(230, 60, 60)); a.name = "Clipped circle";
        var b = fillOnly(d.pathItems.star(X(d, 200), Y(d, 150), 120, 50, 7), rgb(60, 60, 230)); b.name = "Clipped star";
        var g = group(d, "Clip Group", [a, b]);
        var c = g.pathItems.rectangle(Y(d, 50), X(d, 50), 200, 150); c.name = "Mask rect";
        c.move(g, ElementPlacement.PLACEATBEGINNING);
        c.clipping = true; g.clipped = true;
        // Nested clip inside another clip group
        var n1 = fillOnly(rect(d, 320, 20, 260, 260), rgb(250, 200, 50)); n1.name = "Inner fill";
        var ng = group(d, "Inner Clip", [n1]);
        var nc = ng.pathItems.ellipse(Y(d, 60), X(d, 360), 180, 180); nc.name = "Inner mask";
        nc.move(ng, ElementPlacement.PLACEATBEGINNING); nc.clipping = true; ng.clipped = true;
        var bg = fillOnly(rect(d, 300, 0, 300, 300), rgb(40, 40, 40)); bg.name = "Outer background";
        var og = group(d, "Outer Clip", [ng, bg]);
        var oc = og.pathItems.rectangle(Y(d, 10), X(d, 310), 280, 220); oc.name = "Outer mask";
        oc.move(og, ElementPlacement.PLACEATBEGINNING); oc.clipping = true; og.clipped = true;
        save(d, "clipping");
    });

    // 7. Point text, area text, mixed styles
    step("text", function () {
        var d = newDoc(DocumentColorSpace.RGB, 800, 600);
        var arial = app.textFonts.getByName("ArialMT");
        var bold = app.textFonts.getByName("Arial-BoldMT");
        var pt = d.textFrames.pointText([X(d, 50), Y(d, 100)]);
        pt.contents = "Pulley point";
        pt.name = "Point text";
        pt.textRange.characterAttributes.textFont = arial;
        pt.textRange.characterAttributes.size = 48;
        pt.textRange.characterAttributes.fillColor = rgb(20, 20, 30);
        var area = d.textFrames.areaText(rect(d, 50, 150, 300, 150));
        area.contents = "Area text wraps inside its box. Second sentence to make it wrap.";
        area.name = "Area text";
        area.textRange.characterAttributes.textFont = arial;
        area.textRange.characterAttributes.size = 18;
        area.paragraphs[0].paragraphAttributes.justification = Justification.CENTER;
        var mixed = d.textFrames.pointText([X(d, 400), Y(d, 100)]);
        mixed.contents = "Big small RED\rSecond line";
        mixed.name = "Mixed styles";
        var tr = mixed.textRange;
        tr.characterAttributes.textFont = arial; tr.characterAttributes.size = 24; tr.characterAttributes.fillColor = rgb(0, 0, 0);
        var big = mixed.textRange.characters[0]; // "B"
        for (var i = 0; i < 3; i++) { var ch = mixed.textRange.characters[i]; ch.characterAttributes.size = 48; ch.characterAttributes.textFont = bold; }
        for (var k = 10; k < 13; k++) { mixed.textRange.characters[k].characterAttributes.fillColor = rgb(220, 0, 0); }
        mixed.textRange.characterAttributes.tracking = 50;
        mixed.textRange.characterAttributes.autoLeading = false;
        mixed.textRange.characterAttributes.leading = 60;
        var rot = d.textFrames.pointText([X(d, 450), Y(d, 400)]);
        rot.contents = "Rotated";
        rot.name = "Rotated text";
        rot.textRange.characterAttributes.textFont = arial; rot.textRange.characterAttributes.size = 36;
        rot.rotate(15);
        var right = d.textFrames.pointText([X(d, 750), Y(d, 550)]);
        right.contents = "Right aligned";
        right.name = "Right text";
        right.textRange.characterAttributes.textFont = arial; right.textRange.characterAttributes.size = 20;
        right.paragraphs[0].paragraphAttributes.justification = Justification.RIGHT;
        save(d, "text");
    });

    // 8. Rotated, scaled, skewed objects
    step("transforms", function () {
        var d = newDoc(DocumentColorSpace.RGB, 800, 600);
        var r = fillOnly(rect(d, 50, 50, 200, 100), rgb(200, 80, 40)); r.name = "Rotated 30"; r.rotate(30);
        var e = fillOnly(ell(d, 300, 50, 150, 150), rgb(40, 120, 200)); e.name = "Scaled 150x60"; e.resize(150, 60);
        var s = fillOnly(rect(d, 550, 50, 150, 150), rgb(40, 180, 90)); s.name = "Skewed";
        var m = app.getIdentityMatrix(); m.mValueC = 0.4; s.transform(m);
        var a = fillOnly(rect(d, 100, 350, 100, 100), rgb(240, 200, 0)); a.name = "In rotated group A";
        var b = fillOnly(ell(d, 220, 350, 100, 100), rgb(120, 0, 160)); b.name = "In rotated group B";
        var g = group(d, "Rotated group", [a, b]); g.rotate(-20);
        var f = fillOnly(rect(d, 500, 350, 200, 150), rgb(0, 0, 0)); f.name = "Flipped"; f.resize(-100, 100);
        save(d, "transforms");
    });

    // 9. Very large paths
    step("large", function () {
        var d = newDoc(DocumentColorSpace.RGB, 2000, 2000);
        var pts = [];
        for (var i = 0; i < 5000; i++) {
            var a = (i / 5000) * Math.PI * 2;
            var r = 700 + 40 * Math.sin(a * 60);
            pts.push([1000 + Math.cos(a) * r, 1000 + Math.sin(a) * r]);
        }
        fillOnly(poly(d, pts, true), rgb(30, 30, 30)).name = "5000 points";
        save(d, "large");
        var d2 = newDoc(DocumentColorSpace.RGB, 1920, 1080);
        for (var k = 0; k < 500; k++) {
            var cx = 40 + (k % 25) * 75, cy = 40 + Math.floor(k / 25) * 50;
            var p = d2.pathItems.polygon(X(d2, cx), Y(d2, cy), 20, 20);
            fillOnly(p, rgb((k * 7) % 255, (k * 13) % 255, (k * 29) % 255));
        }
        save(d2, "perf-500x20");
    });

    // 10. Unsupported items (must warn, never fail)
    step("unsupported", function () {
        var d = newDoc(DocumentColorSpace.RGB, 600, 400);
        var keep = fillOnly(rect(d, 20, 20, 100, 100), rgb(0, 150, 0)); keep.name = "Supported rect";
        var toRaster = fillOnly(ell(d, 150, 20, 100, 100), rgb(200, 0, 0));
        var ras = d.rasterize(toRaster); ras.name = "Raster";
        var star = fillOnly(d.pathItems.star(X(d, 350), Y(d, 80), 60, 25, 5), rgb(250, 200, 0)); star.name = "Symbol source";
        var sym = d.symbols.add(star); sym.name = "Pulley Star Symbol";
        var inst = d.symbolItems.add(sym); inst.name = "Symbol instance"; inst.position = [X(d, 450), Y(d, 20)];
        star.remove();
        var pat = null;
        for (var i = 0; i < d.swatches.length; i++) { if (d.swatches[i].color.typename === "PatternColor") { pat = d.swatches[i].color; break; } }
        var pr = rect(d, 20, 200, 150, 150); pr.name = "Pattern fill";
        if (pat) { pr.filled = true; pr.fillColor = pat; } else { fillOnly(pr, rgb(128, 128, 128)); log.push("no pattern swatch in new document"); }
        save(d, "unsupported");
    });

    // 10b. Images: marker.png (64 x 32: left half red, right half blue, green 8 x 8 top-left
    // corner) placed linked as is, scaled and rotated, mirrored, inside a clip group, and once
    // embedded.
    step("images", function () {
        var d = newDoc(DocumentColorSpace.RGB, 600, 400);
        var png = new File(outDir + "/marker.png");
        function place(name, x, y) {
            var p = d.placedItems.add();
            p.file = png;
            p.position = [X(d, x), Y(d, y)];
            p.name = name;
            return p;
        }
        place("Plain", 50, 50);
        var sr = place("Scaled rotated", 250, 60);
        sr.resize(200, 200);
        sr.rotate(30);
        var fl = place("Mirrored", 450, 50);
        fl.resize(-100, 100);
        var em = place("Embedded", 50, 250);
        em.embed();
        d.rasterItems[0].name = "Embedded";
        var ci = place("Clipped image", 250, 250);
        ci.resize(300, 300);
        var clip = rect(d, 260, 260, 100, 60);
        var g = group(d, "Image clip group", [clip, ci]);
        g.clipped = true;
        save(d, "images");
    });

    // 11. CMYK document, spot and global colors, gray
    step("colors-cmyk", function () {
        var d = newDoc(DocumentColorSpace.CMYK, 600, 300);
        fillOnly(rect(d, 20, 20, 100, 100), cmyk(0, 100, 100, 0)).name = "CMYK red";
        fillOnly(rect(d, 140, 20, 100, 100), cmyk(75, 68, 67, 90)).name = "Rich black";
        var spot = d.spots.add(); spot.name = "Pulley Spot"; spot.color = cmyk(100, 0, 60, 0); spot.colorType = ColorModel.SPOT;
        var sc = new SpotColor(); sc.spot = spot; sc.tint = 50;
        fillOnly(rect(d, 260, 20, 100, 100), sc).name = "Spot 50%";
        var gl = d.spots.add(); gl.name = "Pulley Global"; gl.color = cmyk(0, 40, 100, 0); gl.colorType = ColorModel.PROCESS;
        var gc = new SpotColor(); gc.spot = gl; gc.tint = 100;
        fillOnly(rect(d, 380, 20, 100, 100), gc).name = "Global process";
        var gray = new GrayColor(); gray.gray = 30;
        fillOnly(rect(d, 500, 20, 80, 100), gray).name = "Gray 30";
        save(d, "colors-cmyk");
    });

    // 12. Hidden and locked members
    step("hidden", function () {
        var d = newDoc(DocumentColorSpace.RGB, 400, 300);
        var a = fillOnly(rect(d, 20, 20, 100, 100), rgb(0, 0, 200)); a.name = "Visible";
        var b = fillOnly(rect(d, 140, 20, 100, 100), rgb(200, 0, 0)); b.name = "Hidden";
        var c = fillOnly(rect(d, 260, 20, 100, 100), rgb(0, 200, 0)); c.name = "Locked";
        var g = group(d, "Group with hidden", [a, b, c]);
        b.hidden = true; c.locked = true;
        save(d, "hidden");
    });

    return PulleyJSON.stringify({ made: made, log: log });
}
