/*
 * Development only: do Illustrator item uuids survive closing and reopening a document, and
 * saving it under another name? Re-push matches After Effects layers to items by uuid.
 * Never writes to the fixture: the "saved" case uses a copy in `copyPath`.
 * Requires PulleyJSON.
 */
function pulleyUuidsOf(doc) {
    var out = [];
    for (var l = 0; l < doc.layers.length; l++) {
        var items = doc.layers[l].pageItems;
        for (var i = 0; i < items.length && out.length < 200; i++) { out.push(String(items[i].uuid)); }
    }
    return out;
}

function pulleyUuidStability(fixturePath, copyPath) {
    var r = {};
    while (app.documents.length > 0) { app.activeDocument.close(SaveOptions.DONOTSAVECHANGES); }
    var doc = app.open(new File(fixturePath));
    r.first = pulleyUuidsOf(doc);
    doc.close(SaveOptions.DONOTSAVECHANGES);
    // Reopen the same, unchanged file.
    doc = app.open(new File(fixturePath));
    r.reopened = pulleyUuidsOf(doc);
    // Save a copy under another name, close, reopen the copy.
    doc.saveAs(new File(copyPath));
    doc.close(SaveOptions.DONOTSAVECHANGES);
    doc = app.open(new File(copyPath));
    r.copy = pulleyUuidsOf(doc);
    doc.close(SaveOptions.DONOTSAVECHANGES);
    r.sameAfterReopen = r.first.join(",") === r.reopened.join(",");
    r.sameInCopy = r.first.join(",") === r.copy.join(",");
    return PulleyJSON.stringify(r);
}
