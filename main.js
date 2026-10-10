/*
  Comportamento do site: idioma, links de download resolvidos pela release
  mais recente do GitHub e a animação do hero (as contas abrindo uma a uma).
*/
(function () {
  "use strict";

  var REPO = "luanmacea/MultiAlt";
  var RELEASES_PAGE = "https://github.com/" + REPO + "/releases";
  var dict = window.RAM_I18N || { en: {}, pt: {} };
  var LANGS = { en: "en", pt: "pt-BR", es: "es" };
  var lang = "en";
  var release = null;

  // ---------- idioma ----------
  // O inglês original de cada trecho é guardado antes da primeira troca, para
  // voltar do português sem precisar duplicar o HTML no dicionário.
  var originals = new Map();
  var attrOriginals = new Map();

  function t(key, vars) {
    var s = (dict[lang] && dict[lang][key]) || (dict.en && dict.en[key]) || key;
    if (vars) Object.keys(vars).forEach(function (k) { s = s.replace("{" + k + "}", vars[k]); });
    return s;
  }

  function applyLang(next) {
    lang = LANGS[next] ? next : "en";
    document.documentElement.lang = LANGS[lang];
    document.querySelectorAll("[data-i18n]").forEach(function (el) {
      if (!originals.has(el)) originals.set(el, el.innerHTML);
      var key = el.getAttribute("data-i18n");
      var tr = lang === "en" ? null : dict[lang][key];
      el.innerHTML = tr != null ? tr : originals.get(el);
    });
    document.querySelectorAll("[data-i18n-attr]").forEach(function (el) {
      var parts = el.getAttribute("data-i18n-attr").split(":");
      var attr = parts[0], key = parts[1];
      if (!attrOriginals.has(el)) attrOriginals.set(el, el.getAttribute(attr));
      var tr = lang === "en" ? null : dict[lang][key];
      el.setAttribute(attr, tr != null ? fromRoot(attr, tr) : attrOriginals.get(el));
    });
    document.querySelectorAll(".lang button").forEach(function (b) {
      b.setAttribute("aria-pressed", String(b.getAttribute("data-lang") === lang));
    });
    renderRelease();
    renderMock();
    if (typeof renderTour === "function") renderTour();
  }

  // Páginas /pt/ e /es/ já chegam traduzidas (geradas por scripts/site/build-locales.ts):
  // o idioma é o da página, e trocar de idioma é ir para o endereço do outro.
  var pageLang = document.documentElement.getAttribute("data-site-lang");
  // Os arquivos moram na raiz; de /pt/ e /es/ eles ficam um nível acima.
  var ROOT = pageLang ? "../" : "";

  // Endereço do i18n.js (escrito a partir da raiz, ex. "pt/varias-contas-roblox/")
  // visto de /pt/ ou /es/ — a mesma regra do rebase de scripts/site/locales.ts.
  function fromRoot(attr, url) {
    if (!ROOT || (attr !== "href" && attr !== "src") || /^(?:[a-z]+:|#|\/|\.\.\/)/i.test(url)) return url;
    return url === "./" ? ROOT : ROOT + url;
  }

  function initialLang() {
    if (pageLang) return pageLang;
    var fromUrl = new URLSearchParams(location.search).get("lang");
    if (fromUrl) return fromUrl.slice(0, 2).toLowerCase();
    try {
      var saved = localStorage.getItem("ram-site-lang");
      if (saved) return saved;
    } catch (e) { /* sem storage: segue pelo idioma do navegador */ }
    var nav = (navigator.language || "en").slice(0, 2).toLowerCase();
    return LANGS[nav] ? nav : "en";
  }

  document.querySelectorAll(".lang button").forEach(function (b) {
    b.addEventListener("click", function () {
      var next = b.getAttribute("data-lang");
      try { localStorage.setItem("ram-site-lang", next); } catch (e) { /* ok */ }
      if (pageLang) {
        if (next !== pageLang) location.href = next === "en" ? "../" : "../" + next + "/";
        return;
      }
      applyLang(next);
    });
  });

  // ---------- release mais recente ----------
  // As releases saem como pre-release (série 0.x), então /releases/latest do
  // GitHub não as enxerga: a lista vem da API e a primeira publicada vale.
  // Desde a 0.1.8 a release traz o MSI (o setup .exe saiu); o portátil só
  // quando o interruptor PUBLISH_PORTABLE do release-v4.yml está ligado — e
  // desde 03/10/2026 ele está desligado. A linha do portátil na tabela só
  // aparece se o arquivo existe (renderRelease). O MSI padrão vem duas vezes:
  // com a versão no nome e como `MultiAlt-Setup.msi` (nome fixo do botão do
  // README) — o da versão ganha, para o arquivo baixado dizer qual versão é.
  // A edição completa (arquivos com `_full-`/`_Full-` no nome) só aparece nos
  // anexos da release do GitHub (decisão do dono, 10/10/2026): o site nunca
  // aponta para ela.
  function pickAssets(r) {
    var by = { msi: null, portable: null };
    var stableMsi = null;
    (r.assets || []).forEach(function (a) {
      var n = a.name;
      if (/^zz-/.test(n) || /\.sig$/.test(n)) return;
      if (/_full-/i.test(n)) return;
      // MultiAlt-Setup.msi desde a troca de nome; o nome antigo vale para as
      // releases de antes dela.
      if (n === "MultiAlt-Setup.msi" || n === "Roblox-Account-Manager-Setup.msi") stableMsi = a;
      else if (/\.msi$/.test(n)) by.msi = a;
      else if (/_portable\.exe$/.test(n)) by.portable = a;
    });
    if (!by.msi) by.msi = stableMsi;
    return by;
  }

  function fmtSize(bytes) {
    return (bytes / 1048576).toFixed(1).replace(".", lang === "en" ? "." : ",") + " MB";
  }

  function versionLabel(r) {
    var v = r.tag_name.replace(/-beta$/, "");
    return r.prerelease ? v + " " + t("dyn.beta") : v;
  }

  // O botão principal passa por /get/<origem>/ (download-source.js), que o
  // Cloudflare Web Analytics conta e que leva ao MultiAlt-Setup.msi da última
  // release. Sem o download-source.js carregado, fica o link direto.
  function setTrackedDownload(msi) {
    var ds = window.MultiAltDownloadSource;
    var href = ds ? "/get/" + ds.current() + "/" : (msi ? msi.browser_download_url : RELEASES_PAGE);
    document.querySelectorAll(".js-dl-msi").forEach(function (a) { a.href = href; });
  }

  function renderRelease() {
    if (!release) {
      document.querySelectorAll(".js-dl-meta").forEach(function (el) { el.textContent = t("dl.metaFallback"); });
      return;
    }
    var std = pickAssets(release);
    var picked = std;

    setTrackedDownload(std.msi);
    document.querySelectorAll(".js-dl-meta").forEach(function (el) {
      el.textContent = std.msi
        ? t("dyn.meta", { version: versionLabel(release), size: fmtSize(std.msi.size) })
        : t("dl.metaFallback");
    });
    document.querySelectorAll(".js-dl").forEach(function (a) {
      var asset = picked[a.getAttribute("data-kind")];
      a.href = asset ? asset.browser_download_url : RELEASES_PAGE;
    });
    // Formato sem arquivo nesta release (o portátil, com o interruptor
    // desligado) some da tabela em vez de mostrar um link para nada.
    document.querySelectorAll(".js-row").forEach(function (tr) {
      tr.hidden = !picked[tr.getAttribute("data-kind")];
    });
    document.querySelectorAll(".js-size").forEach(function (td) {
      var asset = picked[td.getAttribute("data-kind")];
      td.textContent = asset ? fmtSize(asset.size) : "–";
    });
    var date = new Date(release.published_at).toLocaleDateString(lang === "en" ? "en-US" : LANGS[lang], {
      year: "numeric", month: "long", day: "numeric",
    });
    document.querySelectorAll(".js-release-line").forEach(function (el) {
      el.textContent = t("dyn.released", { version: versionLabel(release), date: date });
    });
  }

  function loadRelease() {
    var cacheKey = "ram-site-release";
    try {
      var cached = JSON.parse(sessionStorage.getItem(cacheKey) || "null");
      if (cached && Date.now() - cached.at < 10 * 60 * 1000) {
        release = cached.release;
        renderRelease();
        return;
      }
    } catch (e) { /* sem cache: busca */ }

    fetch("https://api.github.com/repos/" + REPO + "/releases?per_page=10", {
      headers: { Accept: "application/vnd.github+json" },
    })
      .then(function (res) { if (!res.ok) throw new Error(res.status); return res.json(); })
      .then(function (list) {
        var r = list.filter(function (x) { return !x.draft && x.assets && x.assets.length; })[0];
        if (!r) return;
        // Só o que a página usa, para o cache caber folgado.
        release = {
          tag_name: r.tag_name,
          prerelease: r.prerelease,
          published_at: r.published_at,
          assets: r.assets.map(function (a) { return { name: a.name, size: a.size, browser_download_url: a.browser_download_url }; }),
        };
        try { sessionStorage.setItem(cacheKey, JSON.stringify({ at: Date.now(), release: release })); } catch (e) { /* ok */ }
        renderRelease();
      })
      .catch(function () { /* fica o link para a página de releases */ });
  }

  // ---------- hero: contas abrindo uma a uma ----------
  var ACCOUNTS = [
    { name: "Nebula_Main", initials: "NM", c1: "#38bdf8", c2: "#4f46e5" },
    { name: "Nebula_Alt1", initials: "N1", c1: "#22d3ee", c2: "#0e7490" },
    { name: "PetGrinder22", initials: "PG", c1: "#f472b6", c2: "#9d174d" },
    { name: "ObbyRunner_7", initials: "OR", c1: "#facc15", c2: "#b45309" },
    { name: "TradeAlt", initials: "TA", c1: "#4ade80", c2: "#166534" },
    { name: "AfkFarmer09", initials: "AF", c1: "#a78bfa", c2: "#5b21b6" },
  ];
  var SCENES = [
    ["#2b6cb0", "#6aa8de", "#3f8f4a", "#2f6d39"],
    ["#7c3aed", "#c084fc", "#a16207", "#713f12"],
    ["#0f766e", "#5eead4", "#475569", "#334155"],
    ["#b45309", "#fcd34d", "#15803d", "#14532d"],
    ["#1e3a8a", "#60a5fa", "#e2e8f0", "#94a3b8"],
    ["#9d174d", "#f9a8d4", "#4d7c0f", "#365314"],
  ];
  var state = ACCOUNTS.map(function () { return "offline"; });
  var list = document.getElementById("mock-list");
  var clients = document.getElementById("clients");

  function renderMock() {
    if (!list) return;
    list.innerHTML = "";
    ACCOUNTS.forEach(function (a, i) {
      var li = document.createElement("li");
      if (i === 0) li.className = "sel";
      var st = state[i];
      var dot = st === "launching" ? "var(--st-orange)" : st === "ingame" ? "var(--st-green)" : "transparent";
      li.innerHTML =
        '<span class="av" style="background:linear-gradient(135deg,' + a.c1 + "," + a.c2 + ')">' + a.initials +
        '<span class="st" style="background:' + dot + '"></span></span>' +
        '<span class="acc-name">' + a.name + "</span>" +
        '<span class="acc-state ' + st + '">' + (st === "offline" ? "16d" : t("dyn." + st)) + "</span>";
      list.appendChild(li);
    });
    document.getElementById("mock-launched").textContent = state.filter(function (s) { return s === "launching"; }).length;
    document.getElementById("mock-ingame").textContent = state.filter(function (s) { return s === "ingame"; }).length;
  }

  function addClient(i, instant) {
    var a = ACCOUNTS[i], s = SCENES[i % SCENES.length];
    var el = document.createElement("div");
    el.className = "client";
    el.style.setProperty("--x", i * -12 + "px");
    el.style.setProperty("--y", i * 16 + "px");
    el.style.zIndex = String(i);
    el.innerHTML =
      '<div class="client-bar"><img src="' + ROOT + 'assets/icon.svg" alt="">Roblox — ' + a.name + "</div>" +
      '<div class="client-scene" style="--sky-a:' + s[0] + ";--sky-b:" + s[1] + ";--ground:" + s[2] + ";--ground-2:" + s[3] + '"></div>';
    clients.appendChild(el);
    if (instant) el.classList.add("on");
    else requestAnimationFrame(function () { requestAnimationFrame(function () { el.classList.add("on"); }); });
  }

  function runHero() {
    var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce) {
      state = state.map(function () { return "ingame"; });
      ACCOUNTS.forEach(function (_, i) { addClient(i, true); });
      renderMock();
      return;
    }
    var join = document.getElementById("mock-join");
    var step = 0;
    setTimeout(function () {
      join.classList.add("press");
      setTimeout(function () { join.classList.remove("press"); }, 380);
      (function next() {
        if (step >= ACCOUNTS.length) return;
        var i = step++;
        state[i] = "launching";
        renderMock();
        setTimeout(function () {
          state[i] = "ingame";
          renderMock();
          addClient(i, false);
        }, 650);
        setTimeout(next, 520);
      })();
    }, 900);
  }

  // ---------- cofre: texto encriptado de enfeite ----------
  var cipher = document.getElementById("cipher");
  if (cipher) {
    var abc = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    var out = "";
    for (var k = 0; k < 520; k++) out += abc[(k * 7919 + (k % 13) * 104729) % abc.length];
    cipher.textContent = out;
  }

  // ---------- telas: abas que trocam a foto ----------
  // Clique ou setas (cima/baixo na barra, esquerda/direita quando ela vira fileira no celular)
  // trocam a foto. A frase da tela escolhida aparece embaixo da foto no
  // celular, onde a barra não tem espaço para ela.
  var tourTabs = Array.prototype.slice.call(document.querySelectorAll(".tour-tab"));
  var tourView = document.getElementById("tour-view");
  var tourImg = tourView && tourView.querySelector("img");
  var tourLinks = tourView ? tourView.querySelectorAll(".tour-shot, .tour-full") : [];
  var tourDesc = tourView && tourView.querySelector(".tour-caption-desc");
  var tourTimer = null;

  function renderTour() {
    var tab = tourTabs.filter(function (b) { return b.getAttribute("aria-selected") === "true"; })[0];
    if (!tab || !tourImg) return;
    var name = tab.querySelector(".tour-name").textContent;
    tourImg.alt = t("sc.altFor", { name: name });
    if (tourDesc) tourDesc.textContent = tab.querySelector(".tour-desc").textContent;
  }

  function selectTour(tab, focus) {
    if (!tourImg) return;
    var shot = ROOT + "assets/screens/" + tab.getAttribute("data-shot") + ".png";
    tourTabs.forEach(function (b) {
      var on = b === tab;
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
    });
    tourView.setAttribute("aria-labelledby", tab.id);
    for (var i = 0; i < tourLinks.length; i++) tourLinks[i].href = shot;
    if (focus) {
      tab.focus();
      tab.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
    renderTour();
    if (tourImg.getAttribute("src") === shot) return;
    // Troca suave: some, troca a imagem, volta quando ela carregou.
    clearTimeout(tourTimer);
    tourImg.classList.add("is-swapping");
    tourTimer = setTimeout(function () {
      tourImg.onload = function () { tourImg.classList.remove("is-swapping"); };
      tourImg.src = shot;
      if (tourImg.complete) tourImg.classList.remove("is-swapping");
    }, 160);
  }

  tourTabs.forEach(function (b, i) {
    b.addEventListener("click", function () { selectTour(b, false); });
    b.addEventListener("keydown", function (e) {
      var step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
      var target = e.key === "Home" ? 0 : e.key === "End" ? tourTabs.length - 1 : step ? (i + step + tourTabs.length) % tourTabs.length : -1;
      if (target < 0) return;
      e.preventDefault();
      selectTour(tourTabs[target], true);
    });
    // Adianta o download da foto quando a pessoa passa o mouse.
    b.addEventListener("pointerenter", function () {
      var pre = new Image();
      pre.src = ROOT + "assets/screens/" + b.getAttribute("data-shot") + ".png";
    }, { once: true });
  });

  applyLang(initialLang());
  setTrackedDownload(null);
  loadRelease();
  runHero();
})();
