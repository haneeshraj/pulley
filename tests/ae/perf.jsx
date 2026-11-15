/*
 * AE import speed under different viewer states (development only).
 * Requires PulleyJSON, PulleyApply and dump.jsx (pulleyTestNewProject).
 */
function pulleyPerfMode(planPath, mode) {
    try { app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES); } catch (e) {}
    app.newProject();
    var c = app.project.items.addComp("Perf", 1920, 1080, 1, 10, 30);
    var r = { mode: mode, hasDisableRendering: typeof app.disableRendering };
    if (mode !== "closed") { c.openInViewer(); }
    var tmp = null;
    if (mode === "swapViewer") {
        tmp = app.project.items.addComp("Pulley temp", 100, 100, 1, 1, 30);
        tmp.openInViewer();
        // The target comp must stay the active item for the plan interpreter.
        c.selected = true;
    }
    if (mode === "disableRendering") { try { r.before = app.disableRendering; app.disableRendering = true; r.after = app.disableRendering; } catch (eD) { r.disableError = String(eD); } }
    // Make the target comp the active item without showing it.
    if (mode === "closed" || mode === "swapViewer") { app.project.activeItem; }
    var t0 = new Date().getTime();
    var out = pulleyApplyPlanInto(planPath, c);
    r.ms = new Date().getTime() - t0;
    if (mode === "disableRendering") { try { app.disableRendering = false; } catch (eE) {} }
    if (tmp) { c.openInViewer(); tmp.remove(); }
    r.result = out;
    r.layers = c.numLayers;
    return PulleyJSON.stringify(r);
}

/* Build in a hidden temporary comp, then move the layers into the open target comp. */
function pulleyPerfStaged(planPath, howMove) {
    try { app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES); } catch (e) {}
    app.newProject();
    var target = app.project.items.addComp("Target", 1920, 1080, 1, 10, 30);
    target.openInViewer();
    var r = { move: howMove };
    var t0 = new Date().getTime();
    var stage = app.project.items.addComp("Pulley staging", 1920, 1080, 1, 10, 30);
    var out = pulleyApplyPlanInto(planPath, stage);
    r.buildMs = new Date().getTime() - t0;
    var t1 = new Date().getTime();
    if (howMove === "copyToComp") {
        // copyToComp puts each copy at the top: copy bottom-most first to keep the order.
        for (var i = stage.numLayers; i >= 1; i--) { stage.layer(i).copyToComp(target); }
        stage.remove();
    } else {
        target.layers.add(stage);
    }
    r.moveMs = new Date().getTime() - t1;
    r.totalMs = new Date().getTime() - t0;
    r.targetLayers = target.numLayers;
    r.firstName = target.numLayers ? target.layer(1).name : "";
    r.ok = out.indexOf('"ok":true') >= 0;
    return PulleyJSON.stringify(r);
}
