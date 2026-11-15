/*
 * Pulley: JSON *serializer* for ExtendScript (which has no JSON object).
 * ExtendScript never parses JSON in Pulley: inputs arrive as plain string arguments or as the
 * line-based AE plan, so there is no JSON.parse here and no eval anywhere.
 */
var PulleyJSON = (function () {
    function hex4(c) {
        var h = c.charCodeAt(0).toString(16);
        while (h.length < 4) { h = "0" + h; }
        return "\\u" + h;
    }
    function str(s) {
        // Escape quotes and backslashes; escape everything outside printable ASCII so the
        // output is plain ASCII whatever the host's string encoding.
        return '"' + s.replace(/[\\"]/g, function (c) { return "\\" + c; })
            .replace(/[\u0000-\u001f\u007f-￿]/g, hex4) + '"';
    }
    function ser(v) {
        if (v === null || v === undefined) { return "null"; }
        var t = typeof v;
        if (t === "number") { return isFinite(v) ? String(v) : "null"; }
        if (t === "boolean") { return v ? "true" : "false"; }
        if (t === "string") { return str(v); }
        if (v instanceof Array) {
            var a = [];
            for (var i = 0; i < v.length; i++) { a.push(ser(v[i])); }
            return "[" + a.join(",") + "]";
        }
        if (t === "object") {
            var parts = [];
            for (var k in v) {
                if (v.hasOwnProperty(k) && v[k] !== undefined) { parts.push(str(k) + ":" + ser(v[k])); }
            }
            return "{" + parts.join(",") + "}";
        }
        return str(String(v));
    }
    return { stringify: ser };
})();
