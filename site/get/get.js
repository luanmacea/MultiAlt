// Página de passagem do download: o Cloudflare Web Analytics conta esta
// visita pelo caminho (/get/<origem>/) e em seguida o navegador baixa o
// instalador. O meta refresh da página cobre quem estiver sem JavaScript.
(function () {
  var MSI = "https://github.com/luanmacea/MultiAlt/releases/latest/download/MultiAlt-Setup.msi";
  setTimeout(function () { location.replace(MSI); }, 700);
})();
