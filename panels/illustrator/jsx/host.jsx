/*
 * Pulley for Illustrator: ExtendScript entry point (loaded by the CEP manifest).
 * Only fixed functions; the panel calls them with escaped string arguments.
 */
//@include "json.jsx"
//@include "capture.jsx"

/** Selection summary for the panel (counts only; nothing is modified). */
function pulleyAiSelectionInfo() {
    if (app.documents.length === 0) { return PulleyJSON.stringify({ document: false, count: 0 }); }
    var sel = app.activeDocument.selection;
    var n = (sel && sel.typename !== "TextRange") ? sel.length : 0;
    return PulleyJSON.stringify({ document: true, count: n, name: app.activeDocument.name });
}
