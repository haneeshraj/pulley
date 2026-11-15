/*
 * Opens every fixture .ai in `fixturesDir`, selects everything, runs the real Pulley capture
 * (panels/illustrator/jsx/capture.jsx) and writes <name>.capture.json to
 * `outDir`. Also records the empty-selection error and repeated-capture timings.
 * Requires PulleyJSON and PulleyAI to be loaded first.
 */
function pulleyCaptureFixtures(fixturesDir, outDir) {
    var results = [];
    function write(path, text) {
        var f = new File(path);
        f.encoding = "UTF-8";
        f.open("w");
        f.write(text);
        f.close();
    }
    var files = new Folder(fixturesDir).getFiles("*.ai");
    for (var i = 0; i < files.length; i++) {
        var name = decodeURI(files[i].name).replace(/\.ai$/, "");
        var entry = { name: name };
        try {
            var doc = app.open(files[i]);
            entry.savedAfterOpen = doc.saved;
            doc.selection = null;
            app.executeMenuCommand("selectall");
            entry.selected = doc.selection.length;
            // Mark the document clean after selecting (selection alone may mark it changed),
            // so the flag after capture shows whether the capture changed anything.
            doc.saved = true;
            entry.savedBeforeCapture = doc.saved;
            var t0 = new Date().getTime();
            var out = PulleyAI.capture("1", outDir + "/" + name + ".svg");
            entry.ms = new Date().getTime() - t0;
            entry.sourceSavedAfter = doc.saved;
            entry.selectedAfter = doc.selection.length;
            write(outDir + "/" + name + ".capture.json", out);
            entry.ok = out.indexOf('"ok":true') === 0 || out.indexOf('{"ok":true') === 0;
            if (name === "perf-500x20") {
                var times = [];
                for (var r = 0; r < 5; r++) {
                    var t1 = new Date().getTime();
                    PulleyAI.capture("1", outDir + "/repeat.svg");
                    times.push(new Date().getTime() - t1);
                }
                entry.repeatMs = times;
            }
            if (name === "shapes") {
                doc.selection = null;
                write(outDir + "/empty-selection.capture.json", PulleyAI.capture("1", outDir + "/empty.svg"));
            }
            doc.close(SaveOptions.DONOTSAVECHANGES);
        } catch (e) {
            entry.error = e.toString() + (e.line ? " (line " + e.line + ")" : "");
            while (app.documents.length > 0) { app.activeDocument.close(SaveOptions.DONOTSAVECHANGES); }
        }
        results.push(entry);
    }
    return PulleyJSON.stringify(results);
}
