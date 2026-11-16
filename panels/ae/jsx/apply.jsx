/*
 * Pulley: After Effects plan interpreter (ExtendScript). Fixed, pre-written code only.
 *
 * Reads a plan file written by Pulley (format: core/pulley-core/src/aeplan.rs) and maps
 * each line to a few DOM calls. The plan carries only numbers, fixed keywords and
 * percent-encoded strings (decoded with the native decodeURIComponent); nothing from it is
 * ever evaluated. Keywords such as blend modes and justifications are looked up in fixed
 * whitelists below.
 *
 * Arithmetic here is limited to re-centering a text layer's anchor on the text bounds AE
 * measures (Rust can't know AE's text layout): new position = position + M * delta, with M
 * supplied by the plan.
 *
 * Requires PulleyJSON (json.jsx).
 */
var PulleyApply = (function () {
    // Shape-group blend mode values of "ADBE Vector Blend Mode" (verified in tests/ae).
    var GROUP_BLEND = {
        NORMAL: 1, DARKEN: 3, MULTIPLY: 4, COLOR_BURN: 5, LINEAR_BURN: 6, LIGHTEN: 9, SCREEN: 10,
        COLOR_DODGE: 11, LINEAR_DODGE: 12, OVERLAY: 15, SOFT_LIGHT: 16, HARD_LIGHT: 17,
        DIFFERENCE: 23, EXCLUSION: 24, HUE: 26, SATURATION: 27, COLOR: 28, LUMINOSITY: 29
    };
    var LAYER_BLEND_NAMES = {
        NORMAL: 1, DARKEN: 1, MULTIPLY: 1, COLOR_BURN: 1, LINEAR_BURN: 1, LIGHTEN: 1, SCREEN: 1,
        COLOR_DODGE: 1, LINEAR_DODGE: 1, OVERLAY: 1, SOFT_LIGHT: 1, HARD_LIGHT: 1, DIFFERENCE: 1,
        EXCLUSION: 1, HUE: 1, SATURATION: 1, COLOR: 1, LUMINOSITY: 1
    };
    var JUST_NAMES = {
        LEFT_JUSTIFY: 1, CENTER_JUSTIFY: 1, RIGHT_JUSTIFY: 1, FULL_JUSTIFY_LASTLINE_LEFT: 1,
        FULL_JUSTIFY_LASTLINE_CENTER: 1, FULL_JUSTIFY_LASTLINE_RIGHT: 1, FULL_JUSTIFY_LASTLINE_FULL: 1
    };
    var MASK_NAMES = { ADD: 1, SUBTRACT: 1, INTERSECT: 1, DIFFERENCE: 1 };
    var DASH_NAMES = ["ADBE Vector Stroke Dash 1", "ADBE Vector Stroke Gap 1", "ADBE Vector Stroke Dash 2",
        "ADBE Vector Stroke Gap 2", "ADBE Vector Stroke Dash 3", "ADBE Vector Stroke Gap 3"];

    var plugplug = null;

    function readFile(path) {
        var f = new File(path);
        f.encoding = "UTF-8";
        if (!f.open("r")) { throw new Error("The import data could not be read."); }
        var s = f.read();
        f.close();
        return s;
    }

    function progress(done, total) {
        try {
            if (!plugplug) { plugplug = new ExternalObject("lib:PlugPlugExternalObject"); }
            var ev = new CSXSEvent();
            ev.type = "dev.pulley.progress";
            ev.data = done + "/" + total;
            ev.dispatch();
        } catch (e) {}
    }

    function dec(s) { return decodeURIComponent(s); }

    function shapeOf(closed, csv) {
        var p = csv.split(","), v = [], ti = [], to = [];
        for (var i = 0; i + 5 < p.length; i += 6) {
            v.push([+p[i], +p[i + 1]]);
            ti.push([+p[i + 2], +p[i + 3]]);
            to.push([+p[i + 4], +p[i + 5]]);
        }
        var s = new Shape();
        s.vertices = v;
        s.inTangents = ti;
        s.outTangents = to;
        s.closed = closed === "1";
        return s;
    }

    function setLayerBlend(layer, name) {
        if (name !== "NORMAL" && LAYER_BLEND_NAMES[name] === 1) { layer.blendingMode = BlendingMode[name]; }
    }

    // --- Re-push: layers are tagged in their comment with the source document and item ---
    var CONTENT_OPS = { G: 1, E: 1, P: 1, F: 1, S: 1, GS: 1, GF: 1, O: 1 };

    function parseTag(c) {
        if (typeof c !== "string" || c.indexOf("pulley:") !== 0) { return null; }
        var rest = c.substring(7), slash = rest.indexOf("/");
        if (slash < 0) { return null; }
        try { return { doc: decodeURIComponent(rest.substring(0, slash)), item: decodeURIComponent(rest.substring(slash + 1)) }; } catch (e) { return null; }
    }

    /** True if anything inside has keyframes or an enabled expression. */
    function animated(p, depth) {
        if (!p || depth > 30) { return false; }
        try {
            if (p.propertyType === PropertyType.PROPERTY) {
                return p.numKeys > 0 || (p.canSetExpression && p.expressionEnabled);
            }
            for (var i = 1; i <= p.numProperties; i++) { if (animated(p.property(i), depth + 1)) { return true; } }
        } catch (e) {}
        return false;
    }

    function compAnimated(c, depth) {
        if (depth > 8) { return false; }
        for (var i = 1; i <= c.numLayers; i++) {
            var L = c.layer(i);
            if (animated(L, 0)) { return true; }
            if (L.source instanceof CompItem && compAnimated(L.source, depth + 1)) { return true; }
        }
        return false;
    }

    // ExtendScript (ES3) has no Array.prototype.indexOf.
    function addOnce(list, v) {
        for (var i = 0; i < list.length; i++) { if (list[i] === v) { return; } }
        list.push(v);
    }

    /** Set a property unless it is animated (then name it in `kept`). */
    function setStatic(prop, v, kept, label) {
        try {
            if (prop.numKeys > 0 || (prop.canSetExpression && prop.expressionEnabled)) {
                addOnce(kept, label);
                return;
            }
            if (v instanceof Array) {
                var cur = prop.value;
                if (cur instanceof Array && cur.length === 3 && v.length === 2) { v = [v[0], v[1], cur[2]]; }
            }
            prop.setValue(v);
        } catch (e) {
            addOnce(kept, label);
        }
    }

    function setPosition(tr, xy, kept) {
        var p = tr.property("ADBE Position");
        if (p.dimensionsSeparated) {
            setStatic(tr.property("ADBE Position_0"), xy[0], kept, "position");
            setStatic(tr.property("ADBE Position_1"), xy[1], kept, "position");
        } else {
            setStatic(p, xy, kept, "position");
        }
    }

    function propPath(prop) {
        // Indices from the layer down to this property, so it can be found again later
        // (adding siblings invalidates existing references).
        var path = [], p = prop;
        while (p && p.parentProperty) { path.unshift(p.propertyIndex); p = p.parentProperty; }
        return path;
    }

    function fontFor(run) {
        var ps = run.ps, found = [];
        try {
            if (ps) { found = app.fonts.getFontsByPostScriptName(ps); }
            if ((!found || found.length === 0) && run.family) {
                found = app.fonts.getFontsByFamilyNameAndStyleName(run.family, run.style);
            }
        } catch (e) { found = []; }
        if (found && found.length > 0) { return found[0].postScriptName; }
        return null;
    }

    function styleText(target, run, font) {
        target.font = font;
        target.fontSize = run.size;
        target.applyFill = true;
        target.fillColor = run.color;
        target.tracking = run.tracking;
        if (run.leading >= 0) { target.autoLeading = false; target.leading = run.leading; } else { target.autoLeading = true; }
        if (run.shift !== 0) { target.baselineShift = run.shift; }
    }

    /**
     * Apply a plan file. `cancelPath`: if this file exists, stop and remove what was built.
     * Returns JSON text with counts, missing fonts, warnings and timing.
     */
    function apply(planPath, cancelPath, compOverride, ffxPrefix) {
        var t0 = new Date().getTime();
        var res = { ok: false, layers: 0, groups: 0, paths: 0, texts: 0, comps: 0, missingFonts: [], warnings: [], gradients: 0, cancelled: false };
        var text;
        try { text = readFile(planPath); } catch (eR) { res.error = { code: "plan_unreadable", message: String(eR) }; return PulleyJSON.stringify(res); }
        if (text.indexOf("PULLEY-PLAN 1\n") !== 0) {
            res.error = { code: "plan_version", message: "The After Effects panel and its core do not match. Reinstall the Pulley After Effects panel." };
            return PulleyJSON.stringify(res);
        }
        var comp = compOverride || app.project.activeItem;
        if (!(comp instanceof CompItem)) { comp = null; }
        var compStack = [], layerLists = [], containers = [], layer = null, undoOpen = false;
        // AE redraws an open comp after every scripted change, which makes big imports ~10x
        // slower (measured: 500 paths in one layer, 55 s with the comp on screen vs 4 s off
        // screen). For big plans, show a tiny placeholder comp while building, and open new
        // comps only once they are finished.
        var viewSwap = null, openAtEnd = null;
        var gradRefs = [];
        var userSelection = [];
        var createdLayers = [], createdComps = [], missing = {}, textState = null;
        var skipFallback = false, inFallback = false, lineNo = 0, ops = 0;
        // Re-push state: layers tagged by this document in the target comp, by item.
        var docKey = null, updateOn = false, existing = {}, seenKeys = {}, pendingId = null;
        var skipContents = false, skipMasks = false, masksPending = false, skipPre = 0, preStack = [];
        res.updated = 0;
        var total = 0, pos = 0, len = text.length, idx;
        for (idx = text.indexOf("\n"); idx >= 0; idx = text.indexOf("\n", idx + 1)) { total++; }
        if (comp) {
            compStack.push(comp);
            layerLists.push([]);
            try { var sl = comp.selectedLayers; for (var si = 0; si < sl.length; si++) { userSelection.push(sl[si]); } } catch (eSel) {}
        }
        var big = total > 200;
        function hideViewer() {
            if (!big || viewSwap) { return; }
            try {
                if (app.activeViewer && app.activeViewer.type === ViewerType.VIEWER_COMPOSITION) {
                    viewSwap = app.project.items.addComp("Pulley: importing", 4, 4, 1, 1, 1);
                    viewSwap.openInViewer();
                }
            } catch (eV) { viewSwap = null; }
        }

        function top() { return containers[containers.length - 1]; }
        function curComp() { return compStack[compStack.length - 1]; }
        function curList() { return layerLists[layerLists.length - 1]; }
        function warn(msg, noLine) { if (res.warnings.length < 200) { res.warnings.push(noLine ? msg : "line " + lineNo + ": " + msg); } }
        function newLayer(L) { createdLayers.push(L); curList().push(L); res.layers++; }
        function tagFor(item) { return "pulley:" + encodeURIComponent(docKey || "") + "/" + encodeURIComponent(item); }
        function keptWarning(L, kept) {
            if (kept.length) { warn(L.name + ": animated in After Effects, so its " + kept.join(", ") + " kept the animation instead of being updated.", true); }
        }
        /** The existing layer to update for this item (topmost if duplicated), or null. */
        function takeExisting(item) {
            if (item === null) { return null; }
            seenKeys[item] = true;
            var list = existing[item];
            if (!updateOn || !list || !list.length) { return null; }
            if (list.length > 1) { warn(list[0].name + ": " + list.length + " layers carry the same Pulley tag (duplicated in After Effects); only the top one was updated.", true); }
            return list[0];
        }
        function updated(L) { curList().push(L); res.updated++; }

        function nextLine() {
            if (pos >= len) { return null; }
            var nl = text.indexOf("\n", pos);
            if (nl < 0) { nl = len; }
            var l = text.substring(pos, nl);
            pos = nl + 1;
            lineNo++;
            return l;
        }
        function peekLine() {
            var nl = text.indexOf("\n", pos);
            return text.substring(pos, nl < 0 ? len : nl);
        }

        function finishText() {
            var t = textState;
            textState = null;
            var fonts = [], anyMissing = false, i;
            for (i = 0; i < t.runs.length; i++) {
                var f = fontFor(t.runs[i]);
                fonts.push(f);
                if (!f) {
                    anyMissing = true;
                    missing[t.runs[i].ps || (t.runs[i].family + " " + t.runs[i].style)] = true;
                }
            }
            if (anyMissing && peekLine() === "FB") { skipFallback = false; return; }
            skipFallback = true;
            var c = curComp();
            // Re-push: update the text layer from an earlier push, unless its text is animated.
            var old = takeExisting(t.id);
            if (old && !(old instanceof TextLayer)) { old = null; }
            if (old) {
                var oldTd = old.property("ADBE Text Properties").property("ADBE Text Document").value;
                if (!!oldTd.boxText !== t.box) {
                    warn(old.name + ": changed between point and area text, so a new layer was added and the old one was kept.", true);
                    old = null;
                }
            }
            var kept = [], L, textKept = false;
            if (old) {
                L = old;
                textKept = animated(L.property("ADBE Text Properties"), 0);
                if (textKept) { kept.push("text"); }
            } else {
                L = t.box ? c.layers.addBoxText([t.bw, t.bh], t.content) : c.layers.addText(t.content);
                L.name = t.name;
                if (t.id !== null) { L.comment = tagFor(t.id); }
            }
            var tp = L.property("ADBE Text Properties").property("ADBE Text Document");
            if (!textKept) {
                var td = tp.value;
                if (old) {
                    td.text = t.content;
                    if (t.box) { try { td.boxTextSize = [t.bw, t.bh]; } catch (eBox) { warn(L.name + ": the text box size could not be updated.", true); } }
                }
                if (JUST_NAMES[t.just] === 1) { td.justification = ParagraphJustification[t.just]; }
                if (t.runs.length === 1) {
                    styleText(td, t.runs[0], fonts[0] || t.runs[0].ps || td.font);
                } else {
                    for (i = 0; i < t.runs.length; i++) {
                        var r = t.runs[i];
                        styleText(td.characterRange(r.start, r.end), r, fonts[i] || r.ps || td.font);
                    }
                }
                tp.setValue(td);
            }
            var tr = L.property("ADBE Transform Group");
            var anchor = [0, 0];
            if (t.box) {
                try { var bp = tp.value.boxTextPos; anchor = [bp[0], bp[1]]; } catch (eB) {}
            }
            var position = [t.px, t.py];
            // Re-center the anchor on the text's measured bounds, keeping it in place.
            try {
                var r0 = L.sourceRectAtTime(0, false);
                var na = [r0.left + r0.width / 2, r0.top + r0.height / 2];
                var dx = na[0] - anchor[0], dy = na[1] - anchor[1];
                position = [position[0] + t.m[0] * dx + t.m[2] * dy, position[1] + t.m[1] * dx + t.m[3] * dy];
                anchor = na;
            } catch (eS) {}
            if (old) {
                setStatic(tr.property("ADBE Anchor Point"), anchor, kept, "anchor point");
                setPosition(tr, position, kept);
                setStatic(tr.property("ADBE Rotate Z"), t.rot, kept, "rotation");
                setStatic(tr.property("ADBE Scale"), [t.sx, t.sy, 100], kept, "scale");
                setStatic(tr.property("ADBE Opacity"), t.op, kept, "opacity");
                if (LAYER_BLEND_NAMES[t.blend] === 1) { L.blendingMode = BlendingMode[t.blend]; }
                keptWarning(L, kept);
                updated(L);
            } else {
                tr.property("ADBE Anchor Point").setValue(anchor);
                tr.property("ADBE Position").setValue(position);
                if (t.rot !== 0) { tr.property("ADBE Rotate Z").setValue(t.rot); }
                if (t.sx !== 100 || t.sy !== 100) { tr.property("ADBE Scale").setValue([t.sx, t.sy, 100]); }
                if (t.op !== 100) { tr.property("ADBE Opacity").setValue(t.op); }
                setLayerBlend(L, t.blend);
                newLayer(L);
            }
            res.texts++;
        }

        function cancelled() {
            try { return new File(cancelPath).exists; } catch (e) { return false; }
        }

        function rollback() {
            var i;
            for (i = createdLayers.length - 1; i >= 0; i--) { try { createdLayers[i].remove(); } catch (e1) {} }
            for (i = createdComps.length - 1; i >= 0; i--) { try { createdComps[i].remove(); } catch (e2) {} }
        }

        try {
            var line;
            while ((line = nextLine()) !== null) {
                if (line === "") { continue; }
                ops++;
                if (ops % 50 === 0) {
                    progress(lineNo, total);
                    if (cancelled()) {
                        rollback();
                        res.cancelled = true;
                        if (res.updated > 0) { warn("Cancelled after some existing layers were already updated: press Ctrl+Z in After Effects to undo them.", true); }
                        break;
                    }
                }
                var f = line.split(" ");
                var k = f[0];
                if (inFallback && skipFallback && k !== "FBEND") { continue; }
                // An updated precomp whose contents are animated: skip to its matching PREEND.
                if (skipPre > 0) {
                    if (k === "PRE") { skipPre++; continue; }
                    if (k !== "PREEND") { continue; }
                    skipPre--;
                    if (skipPre > 0) { continue; }
                }
                // An updated shape layer whose shapes are animated: keep them, skip the content.
                if (skipContents && CONTENT_OPS[k] === 1) { continue; }
                try {
                    switch (k) {
                        case "PULLEY-PLAN": break;
                        case "DOC": {
                            docKey = dec(f[1]);
                            updateOn = f[2] === "1";
                            if (updateOn && compStack.length === 1) {
                                var target = compStack[0];
                                for (var li = 1; li <= target.numLayers; li++) {
                                    var TL = target.layer(li), tag = parseTag(TL.comment);
                                    if (tag && tag.doc === docKey) { (existing[tag.item] = existing[tag.item] || []).push(TL); }
                                }
                            }
                            break;
                        }
                        case "ID":
                            pendingId = compStack.length === 1 ? dec(f[1]) : null;
                            break;
                        case "UNDO":
                            app.beginUndoGroup(dec(f[1]));
                            undoOpen = true;
                            hideViewer();
                            break;
                        case "COMP": {
                            var nc = app.project.items.addComp(dec(f[5]), Math.round(+f[1]), Math.round(+f[2]), 1, +f[4], +f[3]);
                            createdComps.push(nc);
                            compStack = [nc];
                            layerLists = [[]];
                            res.comps++;
                            openAtEnd = nc;
                            break;
                        }
                        case "PRE": {
                            if (compStack.length === 0) { throw new Error("no comp"); }
                            var pid = pendingId;
                            pendingId = null;
                            var parent = curComp();
                            var pw = Math.max(4, Math.round(+f[1])), ph = Math.max(4, Math.round(+f[2]));
                            var oldPre = takeExisting(pid);
                            if (oldPre && !(oldPre.source instanceof CompItem)) { oldPre = null; }
                            if (oldPre && compAnimated(oldPre.source, 0)) {
                                // Keep the animated contents; the layer itself is updated at PREEND.
                                preStack.push({ reuse: oldPre, id: pid, keptContents: true });
                                skipPre = 1;
                                break;
                            }
                            if (oldPre) {
                                // Rebuild inside the existing precomp (the comp item and the layer stay).
                                var oc = oldPre.source;
                                while (oc.numLayers > 0) { oc.layer(1).remove(); }
                                try { oc.width = pw; oc.height = ph; } catch (eWH) {}
                                compStack.push(oc);
                                layerLists.push([]);
                                preStack.push({ reuse: oldPre, id: pid, keptContents: false });
                                break;
                            }
                            var pc = app.project.items.addComp(dec(f[3]), pw, ph, parent.pixelAspect, parent.duration, parent.frameRate);
                            createdComps.push(pc);
                            compStack.push(pc);
                            layerLists.push([]);
                            preStack.push({ reuse: null, id: pid, keptContents: false });
                            res.comps++;
                            break;
                        }
                        case "PREEND": {
                            var entry = preStack.pop() || { reuse: null, id: null, keptContents: false };
                            var PL;
                            if (entry.reuse) {
                                if (!entry.keptContents) { compStack.pop(); layerLists.pop(); }
                                PL = entry.reuse;
                                var pkept = entry.keptContents ? ["contents"] : [];
                                var uptr = PL.property("ADBE Transform Group");
                                setStatic(uptr.property("ADBE Anchor Point"), [+f[2], +f[3]], pkept, "anchor point");
                                setPosition(uptr, [+f[4], +f[5]], pkept);
                                setStatic(uptr.property("ADBE Opacity"), +f[6], pkept, "opacity");
                                if (LAYER_BLEND_NAMES[f[7]] === 1) { PL.blendingMode = BlendingMode[f[7]]; }
                                keptWarning(PL, pkept);
                                updated(PL);
                            } else {
                                var inner = compStack.pop();
                                layerLists.pop();
                                PL = curComp().layers.add(inner);
                                PL.name = dec(f[1]);
                                var ptr = PL.property("ADBE Transform Group");
                                ptr.property("ADBE Anchor Point").setValue([+f[2], +f[3]]);
                                ptr.property("ADBE Position").setValue([+f[4], +f[5]]);
                                if (+f[6] !== 100) { ptr.property("ADBE Opacity").setValue(+f[6]); }
                                setLayerBlend(PL, f[7]);
                                if (entry.id !== null) { PL.comment = tagFor(entry.id); }
                                newLayer(PL);
                            }
                            layer = PL;
                            break;
                        }
                        case "LS": {
                            if (compStack.length === 0) { throw new Error("no comp"); }
                            var lid = pendingId;
                            pendingId = null;
                            var oldShape = takeExisting(lid);
                            if (oldShape && !(oldShape instanceof ShapeLayer)) { oldShape = null; }
                            if (oldShape) {
                                // Re-push: update in place; animated parts are kept.
                                layer = oldShape;
                                var skept = [];
                                var utr = layer.property("ADBE Transform Group");
                                setStatic(utr.property("ADBE Anchor Point"), [+f[2], +f[3]], skept, "anchor point");
                                setPosition(utr, [+f[4], +f[5]], skept);
                                setStatic(utr.property("ADBE Opacity"), +f[6], skept, "opacity");
                                if (LAYER_BLEND_NAMES[f[7]] === 1) { layer.blendingMode = BlendingMode[f[7]]; }
                                var root = layer.property("ADBE Root Vectors Group");
                                if (animated(root, 0)) {
                                    skept.push("shapes");
                                    skipContents = true;
                                    containers = [];
                                } else {
                                    for (var ri = root.numProperties; ri >= 1; ri--) { root.property(ri).remove(); }
                                    containers = [root];
                                }
                                masksPending = true;
                                keptWarning(layer, skept);
                                updated(layer);
                                break;
                            }
                            layer = curComp().layers.addShape();
                            layer.name = dec(f[1]);
                            if (lid !== null) { layer.comment = tagFor(lid); }
                            var tr = layer.property("ADBE Transform Group");
                            tr.property("ADBE Anchor Point").setValue([+f[2], +f[3]]);
                            tr.property("ADBE Position").setValue([+f[4], +f[5]]);
                            if (+f[6] !== 100) { tr.property("ADBE Opacity").setValue(+f[6]); }
                            setLayerBlend(layer, f[7]);
                            containers = [layer.property("ADBE Root Vectors Group")];
                            newLayer(layer);
                            break;
                        }
                        case "G": {
                            var g = top().addProperty("ADBE Vector Group");
                            g.name = dec(f[1]);
                            if (+f[2] !== 100) { g.property("ADBE Vector Transform Group").property("ADBE Vector Group Opacity").setValue(+f[2]); }
                            if (f[3] !== "NORMAL" && GROUP_BLEND[f[3]]) { g.property("ADBE Vector Blend Mode").setValue(GROUP_BLEND[f[3]]); }
                            containers.push(g.property("ADBE Vectors Group"));
                            res.groups++;
                            break;
                        }
                        case "E":
                            if (containers.length > 1) { containers.pop(); }
                            break;
                        case "P":
                            top().addProperty("ADBE Vector Shape - Group").property("ADBE Vector Shape").setValue(shapeOf(f[1], f[3]));
                            res.paths++;
                            break;
                        case "F": {
                            var fl = top().addProperty("ADBE Vector Graphic - Fill");
                            fl.property("ADBE Vector Fill Color").setValue([+f[2], +f[3], +f[4]]);
                            if (f[1] === "2") { fl.property("ADBE Vector Fill Rule").setValue(2); }
                            if (+f[5] !== 100) { fl.property("ADBE Vector Fill Opacity").setValue(+f[5]); }
                            break;
                        }
                        case "S":
                        case "GS": {
                            var gradient = k === "GS";
                            var st = top().addProperty(gradient ? "ADBE Vector Graphic - G-Stroke" : "ADBE Vector Graphic - Stroke");
                            // S: w r g b op cap join miter dashes offset
                            // GS: w type sx sy ex ey hl ha op cap join miter dashes offset grad#
                            var o = gradient ? 9 : 5;
                            st.property("ADBE Vector Stroke Width").setValue(+f[1]);
                            if (gradient) {
                                st.property("ADBE Vector Grad Type").setValue(+f[2]);
                                st.property("ADBE Vector Grad Start Pt").setValue([+f[3], +f[4]]);
                                st.property("ADBE Vector Grad End Pt").setValue([+f[5], +f[6]]);
                                if (+f[7] !== 0) { st.property("ADBE Vector Grad HiLite Length").setValue(+f[7]); }
                                if (+f[8] !== 0) { st.property("ADBE Vector Grad HiLite Angle").setValue(+f[8]); }
                                gradRefs.push({ index: +f[15], layer: layer, path: propPath(st) });
                            } else {
                                st.property("ADBE Vector Stroke Color").setValue([+f[2], +f[3], +f[4]]);
                            }
                            if (+f[o] !== 100) { st.property("ADBE Vector Stroke Opacity").setValue(+f[o]); }
                            if (f[o + 1] !== "1") { st.property("ADBE Vector Stroke Line Cap").setValue(+f[o + 1]); }
                            if (f[o + 2] !== "1") { st.property("ADBE Vector Stroke Line Join").setValue(+f[o + 2]); }
                            // Miter limit is hidden (and can't be set) unless the join is miter.
                            if (f[o + 2] === "1" && +f[o + 3] !== 4) { st.property("ADBE Vector Stroke Miter Limit").setValue(+f[o + 3]); }
                            if (f[o + 4] !== "-") {
                                var dv = f[o + 4].split(","), dashes = st.property("ADBE Vector Stroke Dashes");
                                for (var di = 0; di < dv.length && di < 6; di++) { dashes.addProperty(DASH_NAMES[di]).setValue(+dv[di]); }
                                if (+f[o + 5] !== 0) { dashes.addProperty("ADBE Vector Stroke Offset").setValue(+f[o + 5]); }
                            }
                            break;
                        }
                        case "GF": {
                            // GF rule type sx sy ex ey hl ha op grad#
                            var gf = top().addProperty("ADBE Vector Graphic - G-Fill");
                            gf.property("ADBE Vector Grad Type").setValue(+f[2]);
                            gf.property("ADBE Vector Grad Start Pt").setValue([+f[3], +f[4]]);
                            gf.property("ADBE Vector Grad End Pt").setValue([+f[5], +f[6]]);
                            if (+f[7] !== 0) { gf.property("ADBE Vector Grad HiLite Length").setValue(+f[7]); }
                            if (+f[8] !== 0) { gf.property("ADBE Vector Grad HiLite Angle").setValue(+f[8]); }
                            if (f[1] === "2") { gf.property("ADBE Vector Fill Rule").setValue(2); }
                            if (+f[9] !== 100) { gf.property("ADBE Vector Fill Opacity").setValue(+f[9]); }
                            gradRefs.push({ index: +f[10], layer: layer, path: propPath(gf) });
                            break;
                        }
                        case "O": {
                            var off = top().addProperty("ADBE Vector Filter - Offset");
                            off.property("ADBE Vector Offset Amount").setValue(+f[1]);
                            off.property("ADBE Vector Offset Line Join").setValue(+f[2]);
                            if (f[2] === "1") { off.property("ADBE Vector Offset Miter Limit").setValue(+f[3]); }
                            break;
                        }
                        case "LT":
                            textState = {
                                name: dec(f[1]), box: f[2] === "1", bw: +f[3], bh: +f[4], px: +f[5], py: +f[6],
                                rot: +f[7], sx: +f[8], sy: +f[9], op: +f[10], blend: f[11], just: f[12],
                                m: [+f[13], +f[14], +f[15], +f[16]], content: "", runs: [], id: pendingId
                            };
                            pendingId = null;
                            break;
                        case "TX":
                            if (textState) { textState.content = dec(f[1]); }
                            break;
                        case "R":
                            if (textState) {
                                textState.runs.push({
                                    start: +f[1], end: +f[2], ps: dec(f[3]), family: dec(f[4]), style: dec(f[5]),
                                    size: +f[6], tracking: +f[7], leading: +f[8], color: [+f[9], +f[10], +f[11]], shift: +f[12]
                                });
                            }
                            break;
                        case "TEND":
                            if (textState) { finishText(); }
                            break;
                        case "FB":
                            inFallback = true;
                            break;
                        case "FBEND":
                            inFallback = false;
                            skipFallback = false;
                            break;
                        case "MATTE": {
                            var list = curList();
                            if (list.length >= 2) { list[list.length - 2].setTrackMatte(list[list.length - 1], TrackMatteType.ALPHA); }
                            break;
                        }
                        case "MASK": {
                            if (!layer || skipMasks) { break; }
                            var parade = layer.property("ADBE Mask Parade");
                            if (masksPending) {
                                // An updated layer: its masks are replaced, unless they are animated.
                                masksPending = false;
                                if (animated(parade, 0)) {
                                    skipMasks = true;
                                    keptWarning(layer, ["masks"]);
                                    break;
                                }
                                for (var mi = parade.numProperties; mi >= 1; mi--) { parade.property(mi).remove(); }
                            }
                            var mk = parade.addProperty("ADBE Mask Atom");
                            mk.property("ADBE Mask Shape").setValue(shapeOf(f[2], f[4]));
                            if (MASK_NAMES[f[1]] === 1) { mk.maskMode = MaskMode[f[1]]; }
                            break;
                        }
                        case "LEND":
                            containers = [];
                            skipContents = false;
                            skipMasks = false;
                            masksPending = false;
                            break;
                        case "END":
                            res.ok = true;
                            break;
                        default:
                            warn("unknown operation");
                    }
                } catch (eOp) {
                    warn(String(eOp));
                    if (k === "LS" || k === "COMP" || k === "PRE") { throw eOp; }
                }
            }
        } catch (e) {
            res.ok = false;
            res.error = { code: "apply_failed", message: String(e) + " (plan line " + lineNo + ")" };
            if (String(e).indexOf("no comp") >= 0) {
                res.error = { code: "no_comp", message: "No composition is active in After Effects." };
            }
        }
        // Layers from an earlier push of this document whose item isn't in this push are kept.
        if (res.ok && updateOn) {
            for (var gone in existing) {
                if (existing.hasOwnProperty(gone) && !seenKeys[gone]) {
                    for (var gi0 = 0; gi0 < existing[gone].length; gi0++) {
                        warn(existing[gone][gi0].name + ": not in this push (removed or hidden in the source), so its layer was kept.", true);
                    }
                }
            }
        }
        // Gradient colors can't be set by script: apply a generated preset to each gradient
        // property. applyPreset works on the selection, so select, apply, deselect, and put
        // the user's layer selection back afterwards.
        if (res.ok && gradRefs.length) {
            var applied = 0, failed = 0;
            for (var gi = 0; gi < gradRefs.length; gi++) {
                var g = gradRefs[gi];
                var ff = ffxPrefix ? new File(ffxPrefix + g.index + ".ffx") : null;
                if (!ff || !ff.exists) { failed++; continue; }
                try {
                    var GL = g.layer, GC = GL.containingComp;
                    var others = GC.selectedLayers;
                    for (var oi = 0; oi < others.length; oi++) { others[oi].selected = false; }
                    GL.selected = true;
                    var sp = GL.selectedProperties;
                    for (var pi = 0; pi < sp.length; pi++) { try { sp[pi].selected = false; } catch (eP) {} }
                    var target = GL;
                    for (var k2 = 0; k2 < g.path.length; k2++) { target = target.property(g.path[k2]); }
                    target.selected = true;
                    GL.applyPreset(ff);
                    target.selected = false;
                    GL.selected = false;
                    applied++;
                } catch (eG) { failed++; warn("Gradient colors: " + String(eG), true); }
            }
            res.gradientsApplied = applied;
            res.gradientsFailed = failed;
            if (failed) { warn(failed + " gradient(s) kept After Effects' default colors.", true); }
        }
        res.gradients = gradRefs.length;
        try { for (var ui = 0; ui < userSelection.length; ui++) { userSelection[ui].selected = true; } } catch (eU2) {}
        try {
            if (openAtEnd && !res.cancelled) { openAtEnd.openInViewer(); }
            else if (viewSwap && comp) { comp.openInViewer(); }
            if (viewSwap) { viewSwap.remove(); }
        } catch (eS) {}
        if (undoOpen) { try { app.endUndoGroup(); } catch (eU) {} }
        for (var m in missing) { if (missing.hasOwnProperty(m)) { res.missingFonts.push(m); } }
        res.ms = new Date().getTime() - t0;
        return PulleyJSON.stringify(res);
    }

    return { apply: apply };
})();

function pulleyApplyPlan(planPath, cancelPath, ffxPrefix) {
    return PulleyApply.apply(planPath, cancelPath, null, ffxPrefix);
}

/** Test hook: apply into a given comp instead of the active one. */
function pulleyApplyPlanInto(planPath, comp, ffxPrefix) {
    return PulleyApply.apply(planPath, planPath + ".cancel", comp, ffxPrefix);
}
