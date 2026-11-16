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
        try { o.keys = p.numKeys; } catch (eK) {}
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
        d.comment = L.comment;
        try { d.effects = L.property("ADBE Effect Parade").numProperties; } catch (eE) {}
        try { d.posKeys = tr.property("ADBE Position").numKeys; } catch (eK) {}
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
function pulleyTestApply(planPath, withComp, outPath, ffxPrefix, saveAs) {
    pulleyTestNewProject(withComp);
    var t0 = new Date().getTime();
    var res = pulleyApplyPlan(planPath, planPath + ".cancel", ffxPrefix || null);
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
    // Gradient colors can only be read back from a saved project (tests/ae/check.mjs parses it).
    if (saveAs) { app.project.save(new File(saveAs)); }
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


/** Save the open project (an .aep path: gradient colors can be read back from it). */
function pulleyTestSaveProject(path) {
    app.project.save(new File(path));
    return "saved";
}

/** Apply a plan into a fresh comp, then Undo once: everything the import built must go away. */
function pulleyTestApplyUndo(planPath, withComp, outPath, ffxPrefix) {
    pulleyTestNewProject(withComp);
    var res = pulleyApplyPlan(planPath, planPath + ".cancel", ffxPrefix || null);
    var count = function () {
        var n = 0;
        for (var i = 1; i <= app.project.numItems; i++) { var it = app.project.item(i); if (it instanceof CompItem) { n += it.numLayers; } }
        return n;
    };
    var u = { layersBefore: count(), itemsBefore: app.project.numItems };
    // The menu item reads "Undo Pulley Import" when the import is the last undo step. Its
    // id is looked up by that text; 16 is Edit > Undo whatever its text is.
    u.namedCommand = app.findMenuCommandId("Undo Pulley Import");
    u.command = u.namedCommand > 0 ? u.namedCommand : 16;
    app.executeCommand(u.command);
    u.layersAfter = count();
    u.itemsAfter = app.project.numItems;
    var comp = app.project.activeItem;
    var text = PulleyJSON.stringify({ result: res, undo: u, dump: (comp instanceof CompItem) ? pulleyDumpComp(comp) : { layers: [] } });
    var f = new File(outPath);
    f.encoding = "UTF-8";
    f.open("w");
    f.write(text);
    f.close();
    return res;
}

/** Select a layer of "the user's", import (gradient presets included), report the selection. */
function pulleyTestSelectionKept(planPath, withComp, outPath, ffxPrefix) {
    pulleyTestNewProject("1");
    var comp = app.project.activeItem;
    var mine = comp.layers.addNull();
    mine.name = "User layer";
    mine.selected = true;
    var res = pulleyApplyPlan(planPath, planPath + ".cancel", ffxPrefix || null);
    var sel = comp.selectedLayers, names = [];
    for (var i = 0; i < sel.length; i++) { names.push(sel[i].name); }
    var text = PulleyJSON.stringify({ result: res, selection: names, dump: pulleyDumpComp(comp) });
    var f = new File(outPath);
    f.encoding = "UTF-8";
    f.open("w");
    f.write(text);
    f.close();
    return res;
}

/** The first top-level layer whose Pulley tag ends with this source item id. */
function pulleyTestLayerByItem(comp, item) {
    for (var i = 1; i <= comp.numLayers; i++) {
        var c = comp.layer(i).comment;
        if (c && c.length >= item.length && c.substring(c.length - item.length) === item) { return comp.layer(i); }
    }
    return null;
}

/**
 * Re-push: apply plan A into a fresh comp, make the edits a user would (rename a layer and add
 * an effect; animate another layer's position), apply plan B into the same comp, dump it, then
 * Undo once (the whole second push) and dump again.
 */
function pulleyTestRepush(planA, planB, outPath, ffxA, ffxB) {
    pulleyTestNewProject("1");
    var comp = app.project.activeItem;
    var resA = pulleyApplyPlan(planA, planA + ".cancel", ffxA || null);
    var rounded = pulleyTestLayerByItem(comp, "4f1c2a");
    var circle = pulleyTestLayerByItem(comp, "4f1c2b");
    var edits = { found: !!rounded && !!circle };
    if (rounded) {
        rounded.name = "My rounded";
        rounded.property("ADBE Effect Parade").addProperty("ADBE Gaussian Blur 2");
    }
    if (circle) {
        var pos = circle.property("ADBE Transform Group").property("ADBE Position");
        pos.setValueAtTime(0, [100, 100]);
        pos.setValueAtTime(1, [300, 300]);
    }
    var resB = pulleyApplyPlan(planB, planB + ".cancel", ffxB || null);
    var dumpB = pulleyDumpComp(comp);
    app.executeCommand(16); // Edit > Undo: the second push only
    var dumpUndo = pulleyDumpComp(comp);
    var text = PulleyJSON.stringify({ result: resB, resultA: resA, edits: edits, dump: dumpB, dumpUndo: dumpUndo });
    var f = new File(outPath);
    f.encoding = "UTF-8";
    f.open("w");
    f.write(text);
    f.close();
    return resB;
}

/** Apply the same plan twice into one comp: the second push must update, not duplicate. */
function pulleyTestPushTwice(planPath, withComp, outPath, ffxPrefix) {
    pulleyTestNewProject("1");
    var comp = app.project.activeItem;
    var first = pulleyApplyPlan(planPath, planPath + ".cancel", ffxPrefix || null);
    var counts = { layers1: comp.numLayers, items1: app.project.numItems };
    var res = pulleyApplyPlan(planPath, planPath + ".cancel", ffxPrefix || null);
    counts.layers2 = comp.numLayers;
    counts.items2 = app.project.numItems;
    var text = PulleyJSON.stringify({ result: res, resultFirst: first, counts: counts, dump: pulleyDumpComp(comp) });
    var f = new File(outPath);
    f.encoding = "UTF-8";
    f.open("w");
    f.write(text);
    f.close();
    return res;
}
