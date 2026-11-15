/*
 * Drives the real Pulley Illustrator extension from the test harness (development only):
 * opens a fixture, selects everything, and asks the Pulley background extension to push by
 * sending it the same CSEvent its panel would.
 */
var pulleyPlugPlug = null;

function pulleyTestOpenSelect(path) {
    while (app.documents.length > 0) { app.activeDocument.close(SaveOptions.DONOTSAVECHANGES); }
    var doc = app.open(new File(path));
    doc.selection = null;
    app.executeMenuCommand("selectall");
    return String(doc.selection.length);
}

function pulleyTestDispatch(type) {
    if (!pulleyPlugPlug) { pulleyPlugPlug = new ExternalObject("lib:PlugPlugExternalObject"); }
    var ev = new CSXSEvent();
    ev.type = type;
    ev.data = "{}";
    ev.dispatch();
    return "sent";
}

function pulleyTestSavedFlag() {
    return app.documents.length ? String(app.activeDocument.saved) : "no document";
}
