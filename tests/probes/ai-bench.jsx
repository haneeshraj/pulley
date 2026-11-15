/*
 * Illustrator micro-benchmark (development only): which DOM operations are slow when run
 * from a CEP extension, and does keeping the temporary document in the background help?
 */
function pulleyAiBench(fixture) {
    var r = {};
    function now() { return new Date().getTime(); }
    var src = app.open(new File(fixture));
    var items = src.layers[0].pageItems;
    var n = items.length, i, t;
    r.items = n;
    var list = [];
    for (i = 0; i < n; i++) { list.push(items[i]); }

    t = now();
    for (i = 0; i < n; i++) { var x = list[i].name; var o = list[i].opacity; var b = list[i].blendingMode; }
    r.readNameOpacityBlend = now() - t;

    t = now();
    for (i = 0; i < n; i++) { var u = list[i].uuid; }
    r.readUuid = now() - t;

    t = now();
    for (i = 0; i < n; i++) { var f = list[i].filled; var c = list[i].fillColor; var rr = c.red; }
    r.readFillColor = now() - t;

    t = now();
    for (i = 0; i < 50; i++) { var pc = list[i].pathPoints.length; }
    r.readPathPointCount50 = now() - t;

    // Temp doc stays in the BACKGROUND: re-activate the source right after creating it.
    t = now();
    var tmp = app.documents.add(src.documentColorSpace, 100, 100);
    r.addDoc = now() - t;
    t = now();
    app.activeDocument = src;
    r.reactivateSource = now() - t;
    var layer = tmp.layers[0];
    t = now();
    var dups = [];
    for (i = 0; i < 100; i++) { dups.push(list[i].duplicate(layer, ElementPlacement.PLACEATEND)); }
    r.duplicate100Background = now() - t;
    t = now();
    for (i = 0; i < 100; i++) { dups[i].name = "pulley" + i; }
    r.rename100Background = now() - t;
    t = now();
    for (i = 0; i < 100; i++) { var nn = dups[i].name; var oo = dups[i].opacity; }
    r.readDup100Background = now() - t;

    // Same with the temp doc ACTIVE.
    app.activeDocument = tmp;
    t = now();
    var dups2 = [];
    for (i = 100; i < 200; i++) { dups2.push(list[i].duplicate(layer, ElementPlacement.PLACEATEND)); }
    r.duplicate100Active = now() - t;
    t = now();
    for (i = 0; i < 100; i++) { dups2[i].name = "pulleyB" + i; }
    r.rename100Active = now() - t;
    app.activeDocument = src;

    // Duplicate everything at once through a temporary group in the TEMP doc: not possible
    // without touching the source; instead measure selection-based copy in one call.
    try {
        t = now();
        app.activeDocument = src;
        for (i = 200; i < 300; i++) { list[i].selected = true; }
        r.select100 = now() - t;
        src.selection = null;
    } catch (eSel) { r.selectError = String(eSel); }

    t = now();
    var f2 = new File(Folder.temp.fsName + "/pulley-bench.svg");
    var opts = new ExportOptionsSVG();
    tmp.exportFile(f2, ExportType.SVG, opts);
    r.exportBackgroundDoc = now() - t;
    r.exportedBytes = f2.length;
    f2.remove();
    r.activeAfterExport = app.activeDocument === src ? "source" : "temp";

    t = now();
    tmp.close(SaveOptions.DONOTSAVECHANGES);
    r.closeTemp = now() - t;

    // Fresh-session PathPoint read of 100 paths x 20 points.
    t = now();
    var cnt = 0;
    for (i = 300; i < 400; i++) {
        var pts = list[i].pathPoints, m = pts.length;
        for (var j = 0; j < m; j++) { var q = pts[j]; var a = q.anchor, l = q.leftDirection, rd = q.rightDirection; cnt += a.length; }
    }
    r.readPathPoints100x20 = now() - t;
    r.sourceSaved = src.saved;
    src.close(SaveOptions.DONOTSAVECHANGES);
    return PulleyJSON.stringify(r);
}
