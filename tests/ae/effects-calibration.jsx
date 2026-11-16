/*
 * Development only: measures how After Effects' Drop Shadow and Gaussian Blur effects behave,
 * so pulley-core can map Figma's shadow and blur values onto them (see docs/VERIFICATION.md,
 * "Effects calibration"). Run through the harness in a fresh project; writes JSON to `outPath`.
 *
 * Pixels are read with sampleImage() from a temporary text layer's Source Text expression
 * (test code only: Pulley itself never sets expressions).
 * Requires PulleyJSON.
 */
function pulleyCalDescribe(fx) {
    var out = [];
    for (var i = 1; i <= fx.numProperties; i++) {
        var p = fx.property(i), d = { name: p.name, matchName: p.matchName };
        try { d.value = p.value; } catch (e1) {}
        try { if (p.hasMin) { d.min = p.minValue; } if (p.hasMax) { d.max = p.maxValue; } } catch (e2) {}
        out.push(d);
    }
    return out;
}

/** A comp of size w x h holding one white solid covering [x0,x1) x [y0,y1). */
function pulleyCalSquare(name, w, h, x0, y0, x1, y1) {
    var c = app.project.items.addComp(name, w, h, 1, 1, 30);
    var s = c.layers.addSolid([1, 1, 1], "square", x1 - x0, y1 - y0, 1, 1);
    s.property("ADBE Transform Group").property("ADBE Position").setValue([(x0 + x1) / 2, (y0 + y1) / 2]);
    return c;
}

/** Channel `ch` (3 = alpha) of layer `layerName` in `comp`, along a row or column, post-effect. */
function pulleyCalSample(comp, layerName, vertical, fixed, from, to, ch) {
    var t = comp.layers.addText("x");
    var expr = "var L = thisComp.layer(\"" + layerName + "\"); var s = [];" +
        "for (var i = " + from + "; i <= " + to + "; i++) {" +
        " var p = " + (vertical ? "[" + fixed + ", i]" : "[i, " + fixed + "]") + ";" +
        " s.push(L.sampleImage(p, [0.5, 0.5], true, 0)[" + ch + "].toFixed(4)); }" +
        " s.join(',');";
    var st = t.property("ADBE Text Properties").property("ADBE Text Document");
    st.expression = expr;
    var v = st.valueAtTime(0, false).text;
    t.remove();
    var parts = v.split(","), out = [];
    for (var i = 0; i < parts.length; i++) { out.push(+parts[i]); }
    return out;
}

function pulleyEffectsCalibration(outPath) {
    var r = {};
    var W = 600, H = 600;
    // Comp-sized precomp of a 200 px white square in the middle (pixels 200..399).
    var inner = pulleyCalSquare("inner", W, H, 200, 200, 400, 400);
    var T = app.project.items.addComp("T", W, H, 1, 1, 30);
    var P = T.layers.add(inner);
    P.name = "P";
    var fxs = P.property("ADBE Effect Parade");

    // 1. Parameters of both effects as After Effects reports them.
    var ds = fxs.addProperty("ADBE Drop Shadow");
    r.dropShadow = pulleyCalDescribe(ds);
    try { ds.property("ADBE Drop Shadow-0001").setValue([1, 0, 0]); r.color3 = "ok"; } catch (eC3) { r.color3 = String(eC3); }
    try { ds.property("ADBE Drop Shadow-0001").setValue([1, 0, 0, 1]); r.color4 = "ok"; } catch (eC4) { r.color4 = String(eC4); }
    r.colorAfter = ds.property("ADBE Drop Shadow-0001").value;
    ds.remove();
    var gb = fxs.addProperty("ADBE Gaussian Blur 2");
    r.gaussianBlur = pulleyCalDescribe(gb);
    gb.remove();

    // 2. Gaussian Blur: alpha across the left edge (x = 199.5) for a few blurriness values.
    // "Repeat Edge Pixels" (on by default) is turned off, as Pulley does: Figma's layer blur
    // fades the edges.
    r.blur = {};
    var values = [5, 10, 15, 20, 30, 40, 60, 80];
    for (var bi = 0; bi < values.length; bi++) {
        var b = fxs.addProperty("ADBE Gaussian Blur 2");
        b.property("ADBE Gaussian Blur 2-0001").setValue(values[bi]);
        b.property("ADBE Gaussian Blur 2-0003").setValue(0);
        r.blur[values[bi]] = pulleyCalSample(T, "P", false, 300, 60, 340, 3);
        b.remove();
    }

    // 3. Drop Shadow softness: shadow only, distance 0, full opacity.
    r.softness = {};
    for (var si = 0; si < values.length; si++) {
        var d = fxs.addProperty("ADBE Drop Shadow");
        d.property("ADBE Drop Shadow-0002").setValue(d.property("ADBE Drop Shadow-0002").maxValue);
        d.property("ADBE Drop Shadow-0004").setValue(0);
        d.property("ADBE Drop Shadow-0005").setValue(values[si]);
        d.property("ADBE Drop Shadow-0006").setValue(1);
        r.softness[values[si]] = pulleyCalSample(T, "P", false, 300, 60, 340, 3);
        d.remove();
    }

    // 4. Opacity scale: half of maxValue should give alpha 0.5 inside the shadow.
    var dh = fxs.addProperty("ADBE Drop Shadow");
    var opMax = dh.property("ADBE Drop Shadow-0002").maxValue;
    dh.property("ADBE Drop Shadow-0002").setValue(opMax / 2);
    dh.property("ADBE Drop Shadow-0004").setValue(0);
    dh.property("ADBE Drop Shadow-0005").setValue(0);
    dh.property("ADBE Drop Shadow-0006").setValue(1);
    r.opacityMax = opMax;
    r.halfOpacityAlpha = pulleyCalSample(T, "P", false, 300, 290, 310, 3);
    dh.remove();

    // 5. Direction and distance: direction 135, distance 50, hard edge, shadow only.
    var dd = fxs.addProperty("ADBE Drop Shadow");
    dd.property("ADBE Drop Shadow-0002").setValue(opMax);
    dd.property("ADBE Drop Shadow-0003").setValue(135);
    dd.property("ADBE Drop Shadow-0004").setValue(50);
    dd.property("ADBE Drop Shadow-0005").setValue(0);
    dd.property("ADBE Drop Shadow-0006").setValue(1);
    r.dirRow = pulleyCalSample(T, "P", false, 300, 180, 460, 3);
    r.dirCol = pulleyCalSample(T, "P", true, 300, 180, 460, 3);
    dd.remove();

    // 6. Rotation: rotate P by 90 degrees, shadow direction 180 (down), distance 50, hard edge.
    // Read T's composite through a parent comp: does the shadow fall below (comp space) or to
    // the left (layer space, rotated with the layer)?
    var dr = fxs.addProperty("ADBE Drop Shadow");
    dr.property("ADBE Drop Shadow-0002").setValue(opMax);
    dr.property("ADBE Drop Shadow-0003").setValue(180);
    dr.property("ADBE Drop Shadow-0004").setValue(50);
    dr.property("ADBE Drop Shadow-0005").setValue(0);
    P.property("ADBE Transform Group").property("ADBE Rotate Z").setValue(90);
    var outer = app.project.items.addComp("outer", W, H, 1, 1, 30);
    var TL = outer.layers.add(T);
    TL.name = "T";
    r.rotBelow = pulleyCalSample(outer, "T", true, 300, 400, 460, 3);
    r.rotLeft = pulleyCalSample(outer, "T", false, 300, 140, 200, 3);
    dr.remove();
    P.property("ADBE Transform Group").property("ADBE Rotate Z").setValue(0);

    // 7. Outside the layer: a 200 x 200 precomp filled by the square, placed in the middle of a
    // 600 x 600 comp. Does a shadow (or blur) render beyond the precomp layer's own bounds?
    var small = pulleyCalSquare("small", 200, 200, 0, 0, 200, 200);
    var T3 = app.project.items.addComp("T3", W, H, 1, 1, 30);
    var S = T3.layers.add(small);
    S.name = "S";
    var so = S.property("ADBE Effect Parade").addProperty("ADBE Drop Shadow");
    so.property("ADBE Drop Shadow-0002").setValue(opMax);
    so.property("ADBE Drop Shadow-0003").setValue(180);
    so.property("ADBE Drop Shadow-0004").setValue(50);
    so.property("ADBE Drop Shadow-0005").setValue(0);
    var outer3 = app.project.items.addComp("outer3", W, H, 1, 1, 30);
    var T3L = outer3.layers.add(T3);
    T3L.name = "T3";
    r.outsideShadow = pulleyCalSample(outer3, "T3", true, 300, 390, 460, 3);
    so.remove();
    var sb = S.property("ADBE Effect Parade").addProperty("ADBE Gaussian Blur 2");
    sb.property("ADBE Gaussian Blur 2-0001").setValue(20);
    sb.property("ADBE Gaussian Blur 2-0003").setValue(0);
    r.outsideBlur = pulleyCalSample(outer3, "T3", true, 300, 390, 430, 3);
    sb.remove();

    var f = new File(outPath);
    f.encoding = "UTF-8";
    f.open("w");
    f.write(PulleyJSON.stringify(r));
    f.close();
    return "ok";
}
