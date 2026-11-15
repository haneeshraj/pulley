/*
 * Pulley verification probe (milestone 1). Not product code.
 * Fixed ExtendScript checks for After Effects and Illustrator. Results are returned
 * as JSON text built by ser() (ExtendScript may not have a JSON object).
 */

function ser(v) {
    if (v === null || v === undefined) { return "null"; }
    var t = typeof v;
    if (t === "number") { return isFinite(v) ? String(v) : "null"; }
    if (t === "boolean") { return v ? "true" : "false"; }
    if (t === "string") {
        return '"' + v.replace(/[\\"]/g, function (c) { return "\\" + c; })
            .replace(/[\u0000-\u001f\u007f-￿]/g, function (c) {
                var h = c.charCodeAt(0).toString(16);
                while (h.length < 4) { h = "0" + h; }
                return "\\u" + h;
            }) + '"';
    }
    if (v instanceof Array) {
        var a = [];
        for (var i = 0; i < v.length; i++) { a.push(ser(v[i])); }
        return "[" + a.join(",") + "]";
    }
    if (t === "object") {
        var parts = [];
        for (var k in v) { if (v.hasOwnProperty(k)) { parts.push(ser(k) + ":" + ser(v[k])); } }
        return "{" + parts.join(",") + "}";
    }
    return ser(String(v));
}

function tryRec(r, name, fn) {
    try { r[name] = fn(); } catch (e) { r[name] = "ERROR: " + e.toString() + (e.line ? " (line " + e.line + ")" : ""); }
}

function childMatchNames(pg) {
    var a = [];
    for (var i = 1; i <= pg.numProperties; i++) {
        var p = pg.property(i);
        a.push(p.matchName + (p.propertyType === PropertyType.PROPERTY ? "" : "/"));
    }
    return a;
}

function enumName(enumObj, value) {
    for (var k in enumObj) { try { if (enumObj[k] === value) { return k; } } catch (e) {} }
    return String(value);
}

function pulleyProbeCaps() {
    var app_ = BridgeTalk.appName;
    if (app_ === "aftereffects") { return pulleyProbeAE(); }
    if (app_ === "illustrator") { return pulleyProbeAI(); }
    return ser({ appName: app_ });
}

/* ------------------------------------------------------------------ After Effects */

function pulleyProbeAE() {
    var r = {};
    tryRec(r, "appVersion", function () { return app.version; });
    tryRec(r, "buildName", function () { return app.buildName; });
    tryRec(r, "esVersion", function () { return $.version; });
    tryRec(r, "typeofJSON", function () { return typeof JSON; });
    tryRec(r, "isoLanguage", function () { return app.isoLanguage; });

    // Fonts API
    tryRec(r, "typeofAppFonts", function () { return typeof app.fonts; });
    tryRec(r, "fontsAllFamilies", function () { return app.fonts.allFonts.length; });
    tryRec(r, "typeofGetFontsByPostScriptName", function () { return typeof app.fonts.getFontsByPostScriptName; });
    tryRec(r, "arialByPostScript", function () {
        var f = app.fonts.getFontsByPostScriptName("ArialMT");
        if (!f || !f.length) { return "none"; }
        return { count: f.length, postScriptName: f[0].postScriptName, familyName: f[0].familyName,
                 styleName: f[0].styleName, isSubstitute: f[0].isSubstitute, technology: String(f[0].technology) };
    });
    tryRec(r, "typeofGetFontsByFamilyNameAndStyleName", function () { return typeof app.fonts.getFontsByFamilyNameAndStyleName; });
    tryRec(r, "missingOrSubstitutedBefore", function () { return app.fonts.missingOrSubstitutedFonts.length; });

    var proj = null, comp = null;
    tryRec(r, "newProject", function () { proj = app.newProject(); return proj !== null; });
    tryRec(r, "addComp", function () {
        comp = app.project.items.addComp("pulley-probe", 1920, 1080, 1, 10, 30);
        $.global.__probeComp = comp;
        return comp.name;
    });
    if (!comp) { return ser(r); }

    app.beginUndoGroup("Pulley Probe");
    var layer, root, grp, contents;
    tryRec(r, "shapeBasics", function () {
        layer = comp.layers.addShape();
        root = layer.property("ADBE Root Vectors Group");
        grp = root.addProperty("ADBE Vector Group");
        grp.name = "probe group";
        contents = grp.property("ADBE Vectors Group");
        var pg = contents.addProperty("ADBE Vector Shape - Group");
        var s = new Shape();
        s.vertices = [[0, 0], [100, 0], [100, 100]];
        s.inTangents = [[0, 0], [0, 0], [-10, 0]];
        s.outTangents = [[0, 0], [10, 0], [0, 0]];
        s.closed = true;
        pg.property("ADBE Vector Shape").setValue(s);
        return { groupChildren: childMatchNames(grp) };
    });

    tryRec(r, "fill", function () {
        var f = contents.addProperty("ADBE Vector Graphic - Fill");
        var rule = f.property("ADBE Vector Fill Rule");
        var before = rule.value;
        rule.setValue(2);
        return { children: childMatchNames(f), ruleDefault: before, ruleAfterSet2: rule.value };
    });

    tryRec(r, "stroke", function () {
        var st = contents.addProperty("ADBE Vector Graphic - Stroke");
        var res = { children: childMatchNames(st) };
        res.lineCapDefault = st.property("ADBE Vector Stroke Line Cap").value;
        res.lineJoinDefault = st.property("ADBE Vector Stroke Line Join").value;
        var dashes = st.property("ADBE Vector Stroke Dashes");
        var errs = [];
        var names = ["ADBE Vector Stroke Dash 1", "ADBE Vector Stroke Gap 1", "ADBE Vector Stroke Dash 2", "ADBE Vector Stroke Offset"];
        for (var i = 0; i < names.length; i++) {
            try { dashes.addProperty(names[i]); } catch (e) { errs.push(names[i] + ": " + e.toString()); }
        }
        // re-fetch after addProperty (references may be invalidated)
        dashes = contents.property(contents.numProperties).property("ADBE Vector Stroke Dashes");
        res.dashChildrenAfterAdd = childMatchNames(dashes);
        res.dashAddErrors = errs;
        return res;
    });

    tryRec(r, "gradientFill", function () {
        var gf = contents.addProperty("ADBE Vector Graphic - G-Fill");
        var colors = gf.property("ADBE Vector Grad Colors");
        var res = { children: childMatchNames(gf) };
        res.colorsValueType = enumName(PropertyValueType, colors.propertyValueType);
        res.colorsCanSetExpression = colors.canSetExpression;
        res.colorsCanVaryOverTime = colors.canVaryOverTime;
        try { res.colorsValue = String(colors.value); } catch (e) { res.colorsValueError = e.toString(); }
        try { colors.setValue([0, 1, 0, 0, 1, 0, 0, 1]); res.colorsSetValue = "no error"; } catch (e2) { res.colorsSetValueError = e2.toString(); }
        try { colors.expression = "[0,1,0,0,1,0,0,1]"; res.colorsSetExpression = "no error"; } catch (e3) { res.colorsSetExpressionError = e3.toString(); }
        res.gradTypeDefault = gf.property("ADBE Vector Grad Type").value;
        return res;
    });

    tryRec(r, "refInvalidation", function () {
        var L = comp.layers.addShape();
        var rt = L.property("ADBE Root Vectors Group");
        var g1 = rt.addProperty("ADBE Vector Group");
        var g2 = rt.addProperty("ADBE Vector Group");
        var res = {};
        try { res.firstGroupNameAfterSiblingAdd = g1.name; } catch (e) { res.firstGroupAfterSiblingAdd = "ERROR: " + e.toString(); }
        try { res.parentStillValid = rt.numProperties; } catch (e2) { res.parentAfterAdd = "ERROR: " + e2.toString(); }
        L.remove();
        return res;
    });

    tryRec(r, "trackMatte", function () {
        var target = comp.layers.addShape();
        var matte = comp.layers.addShape(); // index 1, directly above target
        var res = { typeofSetTrackMatte: typeof target.setTrackMatte };
        target.setTrackMatte(matte, TrackMatteType.ALPHA);
        res.trackMatteLayerIndex = target.trackMatteLayer ? target.trackMatteLayer.index : null;
        res.trackMatteType = enumName(TrackMatteType, target.trackMatteType);
        res.matteLayerEnabledAfter = matte.enabled;
        return res;
    });

    tryRec(r, "text", function () {
        var tl = comp.layers.addText("Pulley probe text");
        var tdProp = tl.property("ADBE Text Properties").property("ADBE Text Document");
        var td = tdProp.value;
        var res = { typeofCharacterRange: typeof td.characterRange, typeofParagraphRange: typeof td.paragraphRange,
                    typeofFontObject: typeof td.fontObject, defaultFont: td.font };
        var cr = td.characterRange(0, 6);
        cr.fontSize = 72;
        cr.fillColor = [1, 0, 0];
        tdProp.setValue(td);
        var td2 = tdProp.value;
        res.sizeInRange = td2.characterRange(0, 1).fontSize;
        res.sizeOutsideRange = td2.characterRange(8, 9).fontSize;
        res.colorInRange = td2.characterRange(0, 1).fillColor;
        try {
            var cr2 = td2.characterRange(7, 12);
            cr2.font = "Arial-BoldMT";
            tdProp.setValue(td2);
            var td3 = tdProp.value;
            res.fontInRange = td3.characterRange(8, 9).font;
            res.fontOutsideRange = td3.characterRange(0, 1).font;
        } catch (e) { res.perRangeFontError = e.toString(); }
        try {
            var td4 = tdProp.value;
            td4.tracking = 50;
            td4.justification = ParagraphJustification.CENTER_JUSTIFY;
            tdProp.setValue(td4);
            var td5 = tdProp.value;
            res.trackingAfterSet = td5.tracking;
            res.justificationAfterSet = enumName(ParagraphJustification, td5.justification);
        } catch (e2) { res.paragraphError = e2.toString(); }
        try {
            var td6 = tdProp.value;
            td6.font = "PulleyNoSuchFont-Regular";
            tdProp.setValue(td6);
            var td7 = tdProp.value;
            res.missingFontReadBack = td7.font;
            res.missingFontIsSubstitute = td7.fontObject ? td7.fontObject.isSubstitute : "no fontObject";
            res.missingOrSubstitutedAfter = app.fonts.missingOrSubstitutedFonts.length;
        } catch (e3) { res.missingFontError = e3.toString(); }
        try {
            var bt = comp.layers.addBoxText([400, 200], "box text");
            var btd = bt.property("ADBE Text Properties").property("ADBE Text Document").value;
            res.boxText = { boxText: btd.boxText, boxTextSize: btd.boxTextSize };
        } catch (e4) { res.boxTextError = e4.toString(); }
        return res;
    });
    app.endUndoGroup();
    return ser(r);
}

function pulleyStore(kind, s) {
    $.global["__probe_" + kind] = s;
    return s.length;
}

function pulleyLen(s) { return s.length; }

function pulleyReadFileLen(path) {
    var f = new File(path);
    f.encoding = "UTF-8";
    if (!f.open("r")) { return "ERROR: open failed: " + f.error; }
    var s = f.read();
    f.close();
    return s.length;
}

/* Recursive-descent JSON parser with no eval (shape of Crockford's json_parse). */
function rdParse(text) {
    var at = 0, ch = text.charAt(0);
    function next() { at += 1; ch = text.charAt(at); return ch; }
    function white() { while (ch === " " || ch === "\n" || ch === "\r" || ch === "\t") { next(); } }
    function number() {
        var start = at;
        if (ch === "-") { next(); }
        while ((ch >= "0" && ch <= "9") || ch === "." || ch === "e" || ch === "E" || ch === "+" || ch === "-") { next(); }
        var n = +text.substring(start, at);
        if (!isFinite(n)) { throw new Error("bad number at " + start); }
        return n;
    }
    function string() {
        var s = "", start;
        next(); start = at;
        while (ch !== '"') {
            if (ch === "") { throw new Error("unterminated string"); }
            if (ch === "\\") {
                s += text.substring(start, at);
                next();
                if (ch === "u") { s += String.fromCharCode(parseInt(text.substr(at + 1, 4), 16)); at += 4; }
                else if (ch === "n") { s += "\n"; } else if (ch === "t") { s += "\t"; } else if (ch === "r") { s += "\r"; }
                else if (ch === "b") { s += "\b"; } else if (ch === "f") { s += "\f"; } else { s += ch; }
                next(); start = at;
            } else { next(); }
        }
        s += text.substring(start, at);
        next();
        return s;
    }
    function value() {
        white();
        if (ch === "{") {
            var o = {}; next(); white();
            if (ch === "}") { next(); return o; }
            while (true) { white(); var k = string(); white(); next(); o[k] = value(); white(); if (ch === "}") { next(); return o; } next(); }
        }
        if (ch === "[") {
            var a = []; next(); white();
            if (ch === "]") { next(); return a; }
            while (true) { a.push(value()); white(); if (ch === "]") { next(); return a; } next(); }
        }
        if (ch === '"') { return string(); }
        if (ch === "t") { at += 3; next(); return true; }
        if (ch === "f") { at += 4; next(); return false; }
        if (ch === "n") { at += 3; next(); return null; }
        return number();
    }
    var result = value();
    return result;
}

function pulleyParseRD() {
    var t = $.hiresTimer;
    var obj = rdParse($.global.__probe_json);
    var us = $.hiresTimer;
    return ser({ micros: us, paths: obj.paths.length });
}

function pulleyParseCSV() {
    var t = $.hiresTimer;
    var parts = $.global.__probe_csv.split(",");
    var idx = 0, n = +parts[idx++], data = [];
    for (var k = 0; k < n; k++) {
        var vc = +parts[idx++], v = [], ti = [], to = [];
        for (var j = 0; j < vc; j++) {
            v.push([+parts[idx], +parts[idx + 1]]);
            ti.push([+parts[idx + 2], +parts[idx + 3]]);
            to.push([+parts[idx + 4], +parts[idx + 5]]);
            idx += 6;
        }
        data.push({ name: "Path " + (k + 1), v: v, i: ti, o: to });
    }
    var us = $.hiresTimer;
    $.global.__probeData = data;
    return ser({ micros: us, paths: data.length });
}

function buildGroup(contentsGroup, d) {
    var g = contentsGroup.addProperty("ADBE Vector Group");
    g.name = d.name;
    var c = g.property("ADBE Vectors Group");
    var pg = c.addProperty("ADBE Vector Shape - Group");
    var s = new Shape();
    s.vertices = d.v; s.inTangents = d.i; s.outTangents = d.o; s.closed = true;
    pg.property("ADBE Vector Shape").setValue(s);
    var f = c.addProperty("ADBE Vector Graphic - Fill");
    f.property("ADBE Vector Fill Color").setValue([0.2, 0.5, 0.8]);
}

function pulleyBuild(mode) {
    var comp = $.global.__probeComp, data = $.global.__probeData;
    if (!comp || !data) { return ser({ error: "missing comp or data" }); }
    app.beginUndoGroup("Pulley Probe Build");
    var t = $.hiresTimer, k;
    try {
        if (mode === "oneLayer") {
            var L = comp.layers.addShape();
            L.name = "bench one layer";
            var root = L.property("ADBE Root Vectors Group");
            for (k = 0; k < data.length; k++) { buildGroup(root, data[k]); }
        } else {
            for (k = 0; k < data.length; k++) {
                var L2 = comp.layers.addShape();
                L2.name = data[k].name;
                buildGroup(L2.property("ADBE Root Vectors Group"), data[k]);
            }
        }
    } catch (e) {
        app.endUndoGroup();
        return ser({ error: e.toString(), at: k });
    }
    var us = $.hiresTimer;
    app.endUndoGroup();
    return ser({ micros: us, items: data.length, layersInComp: comp.numLayers });
}

/* ------------------------------------------------- After Effects micro-benchmarks (run 2) */

function readAll(path) {
    var f = new File(path);
    f.encoding = "UTF-8";
    f.open("r");
    var s = f.read();
    f.close();
    return s;
}

function pulleyMicroAE(linesPath, xmlPath) {
    var r = {}, k, j, t;
    app.newProject();
    var comp = app.project.items.addComp("pulley-micro", 1920, 1080, 1, 10, 30);

    // Decode A: one line per path, split twice (keeps every array small).
    var s = readAll(linesPath);
    t = $.hiresTimer;
    var lines = s.split("\n"), data = [];
    for (k = 0; k < lines.length; k++) {
        if (!lines[k]) { continue; }
        var p = lines[k].split(","), v = [], ti = [], to = [];
        for (j = 0; j < p.length; j += 6) {
            v.push([+p[j], +p[j + 1]]); ti.push([+p[j + 2], +p[j + 3]]); to.push([+p[j + 4], +p[j + 5]]);
        }
        data.push({ name: "Path " + (k + 1), v: v, i: ti, o: to });
    }
    r.decodeLinesMicros = $.hiresTimer;
    r.decodeLinesPaths = data.length;

    // Decode B: native E4X XML parser, one element per path.
    try {
        var xs = readAll(xmlPath);
        t = $.hiresTimer;
        var x = new XML(xs), ps = x.p, n = ps.length(), data2 = [];
        for (k = 0; k < n; k++) {
            var nd = ps[k], q = String(nd.attribute("d")).split(","), v2 = [];
            for (j = 0; j < q.length; j += 6) { v2.push([+q[j], +q[j + 1]]); }
            data2.push({ name: String(nd.attribute("n")), v: v2 });
        }
        r.decodeXmlMicros = $.hiresTimer;
        r.decodeXmlPaths = data2.length;
    } catch (eX) { r.decodeXmlError = eX.toString(); }

    function shapeOf(d) {
        var sh = new Shape();
        sh.vertices = d.v; sh.inTangents = d.i; sh.outTangents = d.o; sh.closed = true;
        return sh;
    }

    // A: per-call cost, 200 groups in one layer.
    var acc = { addGroup: 0, setName: 0, getContents: 0, addPath: 0, setShape: 0, addFill: 0, setColor: 0 };
    app.beginUndoGroup("micro A");
    var L = comp.layers.addShape();
    var root = L.property("ADBE Root Vectors Group");
    t = $.hiresTimer;
    for (k = 0; k < 200; k++) {
        var d = data[k];
        $.hiresTimer;
        var g = root.addProperty("ADBE Vector Group"); acc.addGroup += $.hiresTimer;
        g.name = d.name; acc.setName += $.hiresTimer;
        var c = g.property("ADBE Vectors Group"); acc.getContents += $.hiresTimer;
        var pg = c.addProperty("ADBE Vector Shape - Group"); acc.addPath += $.hiresTimer;
        pg.property("ADBE Vector Shape").setValue(shapeOf(d)); acc.setShape += $.hiresTimer;
        var f = c.addProperty("ADBE Vector Graphic - Fill"); acc.addFill += $.hiresTimer;
        f.property("ADBE Vector Fill Color").setValue([0.2, 0.5, 0.8]); acc.setColor += $.hiresTimer;
    }
    app.endUndoGroup();
    r.perCall200Micros = acc;
    L.remove();

    // B: 500 groups in one layer, cumulative time per 100 (is it quadratic in sibling count?).
    function buildInto(container, d2) {
        var g2 = container.addProperty("ADBE Vector Group");
        g2.name = d2.name;
        var c2 = g2.property("ADBE Vectors Group");
        c2.addProperty("ADBE Vector Shape - Group").property("ADBE Vector Shape").setValue(shapeOf(d2));
        c2.addProperty("ADBE Vector Graphic - Fill").property("ADBE Vector Fill Color").setValue([0.2, 0.5, 0.8]);
    }
    app.beginUndoGroup("micro B");
    var LB = comp.layers.addShape(), rootB = LB.property("ADBE Root Vectors Group"), chunks = [];
    $.hiresTimer;
    for (k = 0; k < 500; k++) { buildInto(rootB, data[k]); if (k % 100 === 99) { chunks.push($.hiresTimer); } }
    app.endUndoGroup();
    r.oneLayerPer100Micros = chunks;
    LB.remove();

    // C: same, no undo group.
    var LC = comp.layers.addShape(), rootC = LC.property("ADBE Root Vectors Group");
    $.hiresTimer;
    for (k = 0; k < 500; k++) { buildInto(rootC, data[k]); }
    r.oneLayerNoUndoMicros = $.hiresTimer;
    LC.remove();

    // D: nested: 25 groups x 20 children (fewer siblings per container).
    app.beginUndoGroup("micro D");
    var LD = comp.layers.addShape(), rootD = LD.property("ADBE Root Vectors Group");
    $.hiresTimer;
    for (k = 0; k < 25; k++) {
        var outer = rootD.addProperty("ADBE Vector Group");
        outer.name = "Group " + k;
        var oc = outer.property("ADBE Vectors Group");
        for (j = 0; j < 20; j++) { buildInto(oc, data[k * 20 + j]); }
    }
    r.nested25x20Micros = $.hiresTimer;
    app.endUndoGroup();
    LD.remove();

    // E: flat paths + fills directly in the layer root (no wrapper group per path).
    app.beginUndoGroup("micro E");
    var LE = comp.layers.addShape(), rootE = LE.property("ADBE Root Vectors Group");
    $.hiresTimer;
    for (k = 0; k < 500; k++) {
        rootE.addProperty("ADBE Vector Shape - Group").property("ADBE Vector Shape").setValue(shapeOf(data[k]));
    }
    r.flat500PathsOnlyMicros = $.hiresTimer;
    app.endUndoGroup();
    LE.remove();

    // F: layer creation: 100 x addShape vs 100 x duplicate of a built template.
    app.beginUndoGroup("micro F");
    $.hiresTimer;
    var made = [];
    for (k = 0; k < 100; k++) { made.push(comp.layers.addShape()); }
    r.addShape100Micros = $.hiresTimer;
    for (k = 0; k < made.length; k++) { made[k].remove(); }
    var tpl = comp.layers.addShape();
    buildInto(tpl.property("ADBE Root Vectors Group"), data[0]);
    $.hiresTimer;
    var dups = [];
    for (k = 0; k < 100; k++) { dups.push(tpl.duplicate()); }
    r.duplicate100Micros = $.hiresTimer;
    for (k = 0; k < 100; k++) {
        dups[k].property("ADBE Root Vectors Group").property(1).property("ADBE Vectors Group").property(1)
            .property("ADBE Vector Shape").setValue(shapeOf(data[k + 1]));
    }
    r.duplicateThenSetShape100Micros = $.hiresTimer;
    app.endUndoGroup();

    // G: 500 layers, one group each, cumulative per 100.
    app.beginUndoGroup("micro G");
    var chunksG = [];
    $.hiresTimer;
    for (k = 0; k < 500; k++) {
        var LG = comp.layers.addShape();
        LG.name = data[k].name;
        buildInto(LG.property("ADBE Root Vectors Group"), data[k]);
        if (k % 100 === 99) { chunksG.push($.hiresTimer); }
    }
    app.endUndoGroup();
    r.manyLayersPer100Micros = chunksG;
    r.layersInComp = comp.numLayers;
    return ser(r);
}

/* ------------------------------------------------------------------ Illustrator */

function pulleyProbeAI() {
    var r = {};
    tryRec(r, "appVersion", function () { return app.version; });
    tryRec(r, "esVersion", function () { return $.version; });
    tryRec(r, "typeofJSON", function () { return typeof JSON; });
    tryRec(r, "locale", function () { return app.locale; });
    tryRec(r, "coordinateSystem", function () { return String(app.coordinateSystem); });
    tryRec(r, "typeofConvertSampleColor", function () { return typeof app.convertSampleColor; });
    tryRec(r, "documentsOpenAtStart", function () { return app.documents.length; });
    var doc = null;
    tryRec(r, "addCmykDoc", function () { doc = app.documents.add(DocumentColorSpace.CMYK, 612, 792); return doc.documentColorSpace.toString(); });
    var purposes = ["defaultpurpose", "exportpurpose", "previewpurpose", "dummypurpose"];
    for (var i = 0; i < purposes.length; i++) {
        (function (p) {
            tryRec(r, "cmykRedToRgb_" + p, function () {
                return app.convertSampleColor(ImageColorSpace.CMYK, [0, 100, 100, 0], ImageColorSpace.RGB, ColorConvertPurpose[p]);
            });
        })(purposes[i]);
    }
    tryRec(r, "richBlackToRgb_default", function () {
        return app.convertSampleColor(ImageColorSpace.CMYK, [75, 68, 67, 90], ImageColorSpace.RGB, ColorConvertPurpose.defaultpurpose);
    });
    tryRec(r, "undoCountsAvailable", function () { return typeof doc.undo; });
    if (doc) { $.global.__probeDoc = doc; }
    return ser(r);
}

function pulleyProbeAIBench() {
    var r = {};
    // Run 1 used a document reference kept in $.global between evalScript calls: pathItems.add() worked
    // but pathItems read back empty. Always re-acquire the document.
    if (app.documents.length === 0) { return ser({ error: "no probe doc" }); }
    var doc = app.activeDocument;
    var N = 500, V = 20, k, j;
    var seed = 12345;
    function rnd() { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; }
    var t = $.hiresTimer;
    try {
        for (k = 0; k < N; k++) {
            var cx = rnd() * 612, cy = rnd() * 792, rad = 5 + rnd() * 30, pts = [];
            for (j = 0; j < V; j++) {
                var a = (j / V) * Math.PI * 2;
                pts.push([cx + Math.cos(a) * rad, cy + Math.sin(a) * rad]);
            }
            var p = doc.pathItems.add();
            p.setEntirePath(pts);
            p.closed = true;
            var pp = p.pathPoints;
            for (j = 0; j < V; j++) {
                var q = pp[j], an = q.anchor;
                q.leftDirection = [an[0] - 3, an[1] + 2];
                q.rightDirection = [an[0] + 3, an[1] - 2];
            }
        }
    } catch (e) { r.createError = e.toString() + " at " + k; }
    r.createMicros = $.hiresTimer;
    try {
        var first = doc.pathItems[0];
        r.typeofUuid = typeof first.uuid;
        r.uuidSample = String(first.uuid);
        r.typeofGetPageItemFromUuid = typeof doc.getPageItemFromUuid;
        if (typeof doc.getPageItemFromUuid === "function") { r.uuidRoundTrip = doc.getPageItemFromUuid(first.uuid) === first; }
    } catch (eU) { r.uuidError = eU.toString(); }

    // Strategy A: DOM traversal into a flat number array.
    t = $.hiresTimer;
    var outArr = [], items = doc.pathItems, n = items.length;
    for (k = 0; k < n; k++) {
        var pts2 = items[k].pathPoints, m = pts2.length;
        for (j = 0; j < m; j++) {
            var q2 = pts2[j], a2 = q2.anchor, l2 = q2.leftDirection, r2 = q2.rightDirection;
            outArr.push(a2[0], a2[1], l2[0], l2[1], r2[0], r2[1]);
        }
    }
    var s = outArr.join(",");
    r.domReadMicros = $.hiresTimer;
    r.domReadPaths = n;
    r.domReadChars = s.length;

    // Strategy B: export the document to SVG in a temp folder.
    try {
        var f = new File(Folder.temp.fsName + "/pulley-probe-" + (new Date()).getTime() + ".svg");
        var opts = new ExportOptionsSVG();
        opts.embedRasterImages = false;
        t = $.hiresTimer;
        doc.exportFile(f, ExportType.SVG, opts);
        r.svgExportMicros = $.hiresTimer;
        r.svgBytes = f.length;
        f.remove();
        r.svgFileRemoved = !f.exists;
    } catch (e2) { r.svgExportError = e2.toString(); }

    try { r.docSavedFlagBeforeClose = doc.saved; } catch (e3) {}
    return ser(r);
}

function pulleyProbeQuit() {
    if (BridgeTalk.appName === "aftereffects") {
        try { app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES); } catch (e) {}
        app.quit();
    } else if (BridgeTalk.appName === "illustrator") {
        try { while (app.documents.length > 0) { app.activeDocument.close(SaveOptions.DONOTSAVECHANGES); } } catch (e2) {}
        app.quit();
    }
    return "ok";
}

/* Illustrator run 3: create and read in ONE evalScript call, with diagnostics. */
function pulleyProbeAIDiag() {
    var r = {}, k, j;
    try {
        var doc = app.documents.add(DocumentColorSpace.RGB, 612, 792);
        r.docsAfterAdd = app.documents.length;
        r.activeIsNew = (app.activeDocument === doc);
        var seed = 12345;
        function rnd() { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; }
        var target = doc.layers[0];
        $.hiresTimer;
        for (k = 0; k < 500; k++) {
            var cx = rnd() * 612, cy = rnd() * 792, rad = 5 + rnd() * 30, pts = [];
            for (j = 0; j < 20; j++) { var a = (j / 20) * Math.PI * 2; pts.push([cx + Math.cos(a) * rad, cy + Math.sin(a) * rad]); }
            var p = target.pathItems.add();
            p.setEntirePath(pts);
            p.closed = true;
            var pp = p.pathPoints;
            for (j = 0; j < 20; j++) { var q = pp[j], an = q.anchor; q.leftDirection = [an[0] - 3, an[1] + 2]; q.rightDirection = [an[0] + 3, an[1] - 2]; }
            if (k === 0) { r.firstPathPoints = p.pathPoints.length; r.layerCountAfterFirst = target.pathItems.length; r.docCountAfterFirst = doc.pathItems.length; }
        }
        r.createMicros = $.hiresTimer;
        r.layerPathItems = target.pathItems.length;
        r.docPathItems = doc.pathItems.length;
        r.docPageItems = doc.pageItems.length;
        r.savedAfterCreate = doc.saved;
        try { r.typeofUuid = typeof doc.pathItems[0].uuid; r.uuidSample = String(doc.pathItems[0].uuid); } catch (eU) { r.uuidError = eU.toString(); }
        try { r.typeofGetPageItemFromUuid = typeof doc.getPageItemFromUuid; } catch (eG) {}

        $.hiresTimer;
        var outArr = [], items = doc.pathItems, n = items.length;
        for (k = 0; k < n; k++) {
            var pts2 = items[k].pathPoints, m = pts2.length;
            for (j = 0; j < m; j++) {
                var q2 = pts2[j], a2 = q2.anchor, l2 = q2.leftDirection, r2 = q2.rightDirection;
                outArr.push(a2[0], a2[1], l2[0], l2[1], r2[0], r2[1]);
            }
        }
        var s = outArr.join(",");
        r.domReadMicros = $.hiresTimer;
        r.domReadPaths = n;
        r.domReadChars = s.length;

        // Second DOM read: does it get slower (PathPoint leak reports)?
        $.hiresTimer;
        var cnt = 0;
        for (k = 0; k < n; k++) { var pts3 = items[k].pathPoints, m3 = pts3.length; for (j = 0; j < m3; j++) { var a3 = pts3[j].anchor; cnt += a3.length; } }
        r.domReadAnchorsOnlyMicros = $.hiresTimer;

        var f = new File(Folder.temp.fsName + "/pulley-probe-" + (new Date()).getTime() + ".svg");
        var opts = new ExportOptionsSVG();
        opts.embedRasterImages = false;
        $.hiresTimer;
        doc.exportFile(f, ExportType.SVG, opts);
        r.svgExportMicros = $.hiresTimer;
        r.svgBytes = f.length;
        f.remove();
        r.savedAfterExport = doc.saved;
    } catch (e) { r.error = e.toString() + " (line " + e.line + ")"; }
    return ser(r);
}
