/*
 * Pulley for After Effects: ExtendScript entry point (loaded by the CEP manifest).
 * Only fixed functions; the panel calls them with escaped string arguments.
 */
//@include "json.jsx"
//@include "apply.jsx"

/** Facts about the active comp, for placement. */
function pulleyAeCompInfo() {
    var c = app.project ? app.project.activeItem : null;
    if (c instanceof CompItem) {
        return PulleyJSON.stringify({ active: true, width: c.width, height: c.height, name: c.name });
    }
    return PulleyJSON.stringify({ active: false });
}
