// Runs synchronously before the app bundle: copies saved settings onto <html> so the first paint already has the
// right theme, direction and density (no light/dark flash). Mirrors applySettings() in src/state/settings.ts.
(function () {
  var s;
  try {
    s = JSON.parse(localStorage.getItem('boors.settings.v1') || 'null');
  } catch {
    s = null;
  }
  var h = document.documentElement;
  var lang = s && s.lang === 'en' ? 'en' : 'fa';
  h.lang = lang;
  h.dir = lang === 'fa' ? 'rtl' : 'ltr';
  h.setAttribute('data-theme', s && s.theme === 'light' ? 'light' : 'dark');
  h.setAttribute('data-colors', s && s.colors === 'cb' ? 'cb' : 'iran');
  var narrow = window.matchMedia && window.matchMedia('(max-width: 700px)').matches;
  h.setAttribute('data-density', s && (s.density === 'lite' || s.density === 'pro') ? s.density : narrow ? 'lite' : 'pro');
})();
