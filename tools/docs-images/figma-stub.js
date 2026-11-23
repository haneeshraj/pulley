// Stands in for the Pulley bridge inside the Figma plugin UI, so the real built UI
// (figma-plugin/dist/ui.html) renders with sample data. render.mjs puts it in front of the UI's
// own scripts. Only used for the pictures in the docs; never shipped.
(function () {
  function FakeBridge() {
    var self = this;
    this.readyState = 0;
    setTimeout(function () { self.readyState = 1; if (self.onopen) { self.onopen(); } }, 5);
  }
  FakeBridge.prototype.reply = function (body) {
    var self = this;
    body.protocol = 1;
    body.id = body.id || 'demo-' + Math.random().toString(36).slice(2);
    setTimeout(function () { if (self.onmessage) { self.onmessage({ data: JSON.stringify(body) }); } }, 5);
  };
  FakeBridge.prototype.send = function (text) {
    var m;
    try { m = JSON.parse(text); } catch (e) { return; }
    if (m.type === 'hello') {
      this.reply({ type: 'welcome', bridgeVersion: '0.1.0', instance: 'demo' });
      this.reply({ type: 'status', clients: [{ role: 'ae' }, { role: 'figma' }] });
    } else if (m.type === 'push_scene') {
      // Answer like After Effects would, passing on the converter's own warnings (as the After
      // Effects extension does).
      var warnings = ((m.scene && m.scene.warnings) || []).map(function (w) { return (w.itemName ? w.itemName + ': ' : '') + w.message; });
      this.reply({ type: 'import_result', transfer: m.transfer,
        result: { ok: true, layers: 9, groups: 0, paths: 9, texts: 1, warnings: warnings, timings: { total: 700 } } });
    }
  };
  FakeBridge.prototype.close = function () {
    this.readyState = 3;
    if (this.onclose) { this.onclose(); }
  };
  window.WebSocket = FakeBridge;
})();
