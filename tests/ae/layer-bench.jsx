/*
 * Development only: what one new shape layer costs in After Effects with each way of building
 * it, while the target comp is open (as when you import into your comp). Run through the
 * harness (tests/harness); writes JSON to outPath. Results are in docs/VERIFICATION.md
 * ("Import speed").
 * Requires PulleyJSON.
 */
function pulleyLayerBench(outPath) {
    var r = {};
    function now() { return new Date().getTime(); }
    function fill(L) {
        var g = L.property("ADBE Root Vectors Group").addProperty("ADBE Vector Group");
        var s = new Shape(); s.vertices = [[0, 0], [10, 0], [10, 10]]; s.closed = true;
        g.property("ADBE Vectors Group").addProperty("ADBE Vector Shape - Group").property("ADBE Vector Shape").setValue(s);
        L.property("ADBE Transform Group").property("ADBE Position").setValue([100, 100]);
    }
    function scenario(label, n, build) {
        var target = app.project.items.addComp("target " + label, 1920, 1080, 1, 10, 30);
        target.openInViewer();
        for (var w = 0; w < 5; w++) { target.layers.addSolid([0.5, 0.5, 0.5], "existing", 100, 100, 1, 10); }
        var t0 = now();
        var ph = app.project.items.addComp("Pulley: importing", 4, 4, 1, 1, 1);
        ph.openInViewer();
        build(target, n);
        target.openInViewer();
        ph.remove();
        r[label] = { total: now() - t0, perLayer: (now() - t0) / n, layers: target.numLayers };
    }
    app.beginUndoGroup("bench3");
    // A: addShape into the (hidden) target.
    scenario("addShape", 200, function (c, n) { for (var i = 0; i < n; i++) { fill(c.layers.addShape()); } });
    // B: duplicate an empty template at the top, move each copy to the top.
    scenario("dupTemplate", 200, function (c, n) {
        var tpl = c.layers.addShape();
        for (var i = 0; i < n; i++) { var d = tpl.duplicate(); d.moveToBeginning(); fill(d); }
        tpl.remove();
    });
    // C: build in a temporary comp that is never opened, then copy each layer into the target.
    scenario("tempCopy", 200, function (c, n) {
        var tmp = app.project.items.addComp("tmp", c.width, c.height, 1, c.duration, c.frameRate);
        var tpl = tmp.layers.addShape();
        for (var i = 0; i < n; i++) { var d = tpl.duplicate(); d.moveToBeginning(); fill(d); }
        tpl.remove();
        var t1 = now();
        for (var k = tmp.numLayers; k >= 1; k--) { tmp.layer(k).copyToComp(c); }
        r.tempCopyCopyOnly = (now() - t1) / n;
        tmp.remove();
    });
    // D: 20 layers into an open comp without the swap (small imports today).
    var small = app.project.items.addComp("small", 1920, 1080, 1, 10, 30);
    small.openInViewer();
    var t0 = now();
    for (var i = 0; i < 20; i++) { fill(small.layers.addShape()); }
    r.smallOpen = { total: now() - t0, perLayer: (now() - t0) / 20 };
    // E: 20 layers with the swap and template duplicates.
    scenario("smallSwapDup", 20, function (c, n) {
        var tpl = c.layers.addShape();
        for (var i = 0; i < n; i++) { var d = tpl.duplicate(); d.moveToBeginning(); fill(d); }
        tpl.remove();
    });
    app.endUndoGroup();
    var f = new File(outPath); f.encoding = "UTF-8"; f.open("w"); f.write(PulleyJSON.stringify(r)); f.close();
    return "ok";
}
