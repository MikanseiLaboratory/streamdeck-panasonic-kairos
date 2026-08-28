(function () {
  function settingEl(name) {
    return document.querySelector('[setting="' + name + '"]');
  }

  function getSetting(name) {
    var el = settingEl(name);
    if (!el) return '';
    var value = el.value;
    if (value === true || value === false) return value;
    return value == null ? '' : String(value);
  }

  function setSetting(name, value) {
    var el = settingEl(name);
    if (el) el.value = value;
  }

  function pickValue() {
    var el = document.getElementById('connection_pick');
    if (el && el.value != null && String(el.value) !== '') {
      return String(el.value);
    }
    return String(getSetting('connection_pick') || 'manual');
  }

  function applyConnectionUi() {
    var value = pickValue();
    var manual = value === 'manual' || value === '';
    var box = document.getElementById('manual-connection');
    if (box) box.style.display = manual ? '' : 'none';
  }

  function syncFromPick() {
    var value = pickValue();
    if (value === 'manual' || value === '') {
      applyConnectionUi();
      return;
    }
    var parts = value.split('\t');
    setSetting('host', parts[0] || '');
    setSetting('port', parts[1] || '');
    setSetting('password', parts[2] || '');
    setSetting('https', parts[3] === '1');
    if (parts.length > 4) setSetting('tcp_port', parts[4] || '');
    applyConnectionUi();
  }

  function updateStatus(payload) {
    var statusEl = document.getElementById('status');
    if (!statusEl) return;
    var connected = !!payload.connected;
    statusEl.textContent = payload.status || (connected ? 'Connected to KAIROS' : 'Not connected');
    statusEl.className = 'pi-status ' + (connected ? 'connected' : 'disconnected');
  }

  var pick = document.getElementById('connection_pick');
  if (pick) {
    pick.addEventListener('change', syncFromPick);
    pick.addEventListener('valuechange', syncFromPick);
  }

  if (window.SDPIComponents && SDPIComponents.streamDeckClient) {
    SDPIComponents.streamDeckClient.sendToPropertyInspector.subscribe(function (event) {
      var payload = event && event.payload;
      if (!payload || payload.event !== 'kairos_state') return;
      updateStatus(payload);
      applyConnectionUi();
    });
    if (SDPIComponents.streamDeckClient.didReceiveSettings) {
      SDPIComponents.streamDeckClient.didReceiveSettings.subscribe(function () {
        applyConnectionUi();
      });
    }
  }

  applyConnectionUi();
})();
