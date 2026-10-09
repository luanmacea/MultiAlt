// De onde a pessoa chegou ao site, para o botão de download levar a
// /get/<origem>/. Cada origem é uma página própria porque o Cloudflare Web
// Analytics conta por caminho (o ?ref= some do relatório). A página /get/
// conta a visita e manda para o instalador. Toda origem daqui precisa de uma
// pasta em site/get/ (scripts/site/downloadSource.test.ts confere).
(function (root) {
  var SOURCES = ["youtube", "google", "bing", "search", "github", "devto", "alternativeto", "direct", "other"];

  var HOSTS = [
    [/(^|\.)youtube\.com$|(^|\.)youtu\.be$/, "youtube"],
    [/(^|\.)google\.[a-z.]+$/, "google"],
    [/(^|\.)bing\.com$/, "bing"],
    [/(^|\.)(duckduckgo\.com|search\.brave\.com|yahoo\.com|yandex\.[a-z]+|ecosia\.org)$/, "search"],
    [/(^|\.)github\.com$/, "github"],
    [/(^|\.)dev\.to$/, "devto"],
    [/(^|\.)alternativeto\.net$/, "alternativeto"],
  ];

  function fromLanding(referrer, search, host) {
    var m = /[?&]ref=([a-z]+)/.exec(search || "");
    if (m && SOURCES.indexOf(m[1]) !== -1) return m[1];
    var refHost = "";
    try { refHost = referrer ? new URL(referrer).hostname : ""; } catch (e) { refHost = ""; }
    if (!refHost || refHost === host) return "direct";
    for (var i = 0; i < HOSTS.length; i++) if (HOSTS[i][0].test(refHost)) return HOSTS[i][1];
    return "other";
  }

  // A primeira página da visita decide: quem chega pelo YouTube e depois
  // troca de idioma continua contando como YouTube.
  function current() {
    var key = "multialt-src";
    try {
      var saved = sessionStorage.getItem(key);
      if (saved && SOURCES.indexOf(saved) !== -1) return saved;
    } catch (e) { /* sem sessionStorage: calcula de novo */ }
    var src = fromLanding(document.referrer, location.search, location.hostname);
    try { sessionStorage.setItem(key, src); } catch (e) { /* ok */ }
    return src;
  }

  var api = { SOURCES: SOURCES, fromLanding: fromLanding, current: current };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.MultiAltDownloadSource = api;
})(this);
