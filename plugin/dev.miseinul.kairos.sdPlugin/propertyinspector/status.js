(function () {
  var statusEl = document.getElementById('status');
  function updateStatus(connected) {
    if (!statusEl) return;
    statusEl.textContent = connected ? 'Connected to KAIROS' : 'Not connected';
    statusEl.className = 'pi-status ' + (connected ? 'connected' : 'disconnected');
  }
  if (window.SDPIComponents && SDPIComponents.streamDeckClient) {
    SDPIComponents.streamDeckClient.sendToPropertyInspector.subscribe(function (event) {
      var payload = event && event.payload;
      if (!payload || payload.event !== 'kairos_state') return;
      updateStatus(!!payload.connected);
    });
  }
})();
