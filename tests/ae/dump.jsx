/*
 * Pulley AE test runner helpers (development only). Applies plans in a fresh project and
 * dumps what was built, so tests/ae/check.mjs can compare it with expectations.
 * Requires PulleyJSON and PulleyApply.
 */
function pulleyTestNewProject(withComp) {
    // Close without saving first: newProject() on a modified project opens a modal dialog.
    try { app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES); } catch (e) {}
    app.newProject();
    if (withComp === "1") {
        var c = app.project.items.addComp("Pulley Test", 1920, 1080, 1, 10, 30);
        c.openInViewer();
    }
    return "ok";
}

function pulleyDumpProp(p, depth) {
    var o = { mn: p.matchName, name: p.name };
    if (p.propertyType === PropertyType.PROPERTY) {
        try {
            var v = p.value;
            if (p.propertyValueType === PropertyValueType.SHAPE) {
                o.closed = v.closed;
                o.n = v.vertices.length;
                o.v0 = v.vertices.length ? v.vertices[0] : null;
                o.in0 = v.inTangents.length ? v.inTangents[0] : null;
                o.out0 = v.outTangents.length ? v.outTangents[0] : null;
            } else if (p.propertyValueType !== PropertyValueType.NO_VALUE && p.propertyValueType !== PropertyValueType.CUSTOM_VALUE) {
                o.value = v;
            }
        } catch (e) { o.err = String(e); }
        return o;
    }
    o.children = [];
    if (depth > 12) { return o; }
    for (var i = 1; i <= p.numProperties; i++) {
        var c = p.property(i);
        // Skip untouched defaults that add noise.
        var mn = c.matchName;
        if (mn === "ADBE Vector Materials Group" || mn === "ADBE Vector Stroke Taper" || mn === "ADBE Vector Stroke Wave") { continue; }
        o.children.push(pulleyDumpProp(c, depth + 1));
    }
    return o;
}

function pulleyDumpComp(comp) {
    var out = { name: comp.name, width: comp.width, height: comp.height, layers: [] };
    for (var i = 1; i <= comp.numLayers; i++) {
        var L = comp.layer(i);
        var tr = L.property("ADBE Transform Group");
        var d = {
            index: i, name: L.name, kind: (L instanceof ShapeLayer) ? "shape" : (L instanceof TextLayer ? "text" : "av"),
            anchor: tr.property("ADBE Anchor Point").value, position: tr.property("ADBE Position").value,
            opacity: tr.property("ADBE Opacity").value, rotation: tr.property("ADBE Rotate Z").value,
            scale: tr.property("ADBE Scale").value, blend: String(L.blendingMode), enabled: L.enabled
        };
        try { d.trackMatte = L.trackMatteLayer ? L.trackMatteLayer.name : null; } catch (eT) {}
        try { d.masks = L.property("ADBE Mask Parade").numProperties; } catch (eM) {}
        if (d.kind === "shape") { d.contents = pulleyDumpProp(L.property("ADBE Root Vectors Group"), 0).children; }
        if (d.kind === "text") {
            var td = L.property("ADBE Text Properties").property("ADBE Text Document").value;
            d.text = td.text;
            d.font = td.font;
            d.fontSize = td.fontSize;
            d.boxText = td.boxText;
            d.justification = String(td.justification);
            d.runs = [];
            try {
                var n = td.text.length;
                for (var c = 0; c < n; c++) {
                    var cr = td.characterRange(c, c + 1);
                    d.runs.push([cr.font, cr.fontSize, cr.fillColor, cr.tracking]);
                }
            } catch (eR) { d.runsError = String(eR); }
            try { var r = L.sourceRectAtTime(0, false); d.rect = [r.left, r.top, r.width, r.height]; } catch (eS) {}
        }
        if (L.source && L.source instanceof CompItem) { d.precomp = pulleyDumpComp(L.source); }
        out.layers.push(d);
    }
    return out;
}

/** Apply a plan into a fresh project (with or without an active comp) and dump the result. */
function pulleyTestApply(planPath, withComp, outPath) {
    pulleyTestNewProject(withComp);
    var t0 = new Date().getTime();
    var res = pulleyApplyPlan(planPath, planPath + ".cancel");
    var ms = new Date().getTime() - t0;
    var comp = app.project.activeItem;
    var dump = null;
    if (!(comp instanceof CompItem)) {
        for (var i = 1; i <= app.project.numItems; i++) { if (app.project.item(i) instanceof CompItem) { comp = app.project.item(i); break; } }
    }
    if (comp instanceof CompItem) {
        // Top-level comp: the one that is not used inside another.
        for (var j = 1; j <= app.project.numItems; j++) {
            var it = app.project.item(j);
            if (it instanceof CompItem && it.usedIn.length === 0) { comp = it; break; }
        }
        dump = pulleyDumpComp(comp);
    }
    var text = PulleyJSON.stringify({ result: res, totalMs: ms, dump: dump });
    var f = new File(outPath);
    f.encoding = "UTF-8";
    f.open("w");
    f.write(text);
    f.close();
    return res;
}

/** Dump the active comp (after imports done by the real Pulley extension). */
function pulleyTestDumpActive(outPath) {
    var comp = app.project.activeItem;
    var text = PulleyJSON.stringify({ dump: (comp instanceof CompItem) ? pulleyDumpComp(comp) : null, items: app.project.numItems });
    var f = new File(outPath);
    f.encoding = "UTF-8";
    f.open("w");
    f.write(text);
    f.close();
    return "ok";
}
