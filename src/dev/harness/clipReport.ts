/**
 * Detector de conteúdo cortado, para a auditoria de telas pequenas
 * (`bun run ui:audit`, e à mão pelo console: `__harness.clipReport()`).
 *
 * Acha duas coisas que só aparecem com a janela menor que o monitor do dono:
 * - **cortado:** elemento com `overflow: hidden/clip` cujo conteúdo é maior que
 *   a caixa (texto com reticências de propósito não conta);
 * - **inalcançável:** texto que termina fora da janela sem nenhum ancestral que
 *   role até ele — o usuário não tem como ver.
 *
 * Mede, não conserta: quem decide se o corte é bug é quem lê o relatório.
 */
export interface ClipFinding {
  kind: "clipY" | "clipX" | "offscreenY" | "offscreenX";
  selector: string;
  text: string;
  detail: string;
}

function describe(el: Element): string {
  const parts: string[] = [];
  let cur: Element | null = el;
  for (let i = 0; cur && i < 3; i++, cur = cur.parentElement) {
    const cls = (cur.getAttribute("class") ?? "").split(/\s+/).filter(Boolean).slice(0, 4).join(".");
    parts.unshift(cur.tagName.toLowerCase() + (cls ? "." + cls : ""));
  }
  return parts.join(" > ");
}

function scrollsTo(el: Element, axis: "x" | "y"): boolean {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const cs = getComputedStyle(p);
    const ov = axis === "y" ? cs.overflowY : cs.overflowX;
    const more = axis === "y" ? p.scrollHeight > p.clientHeight + 1 : p.scrollWidth > p.clientWidth + 1;
    if ((ov === "auto" || ov === "scroll") && more) return true;
  }
  const doc = document.scrollingElement;
  if (doc) {
    const more = axis === "y" ? doc.scrollHeight > doc.clientHeight + 1 : doc.scrollWidth > doc.clientWidth + 1;
    const cs = getComputedStyle(document.body);
    const ov = axis === "y" ? cs.overflowY : cs.overflowX;
    if (more && ov !== "hidden" && ov !== "clip") return true;
  }
  return false;
}

/** Texto visível do próprio elemento (sem o dos filhos). */
function ownText(el: Element): string {
  let t = "";
  for (const n of el.childNodes) if (n.nodeType === Node.TEXT_NODE) t += n.textContent ?? "";
  return t.trim();
}

function visible(el: Element): boolean {
  const cs = getComputedStyle(el);
  if (cs.display === "none" || cs.visibility === "hidden" || Number(cs.opacity) === 0) return false;
  const r = el.getBoundingClientRect();
  return r.width > 0 && r.height > 0;
}

export function clipReport(): ClipFinding[] {
  const out: ClipFinding[] = [];
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const seen = new Set<string>();
  const push = (f: ClipFinding) => {
    const key = f.kind + f.selector + f.text;
    if (!seen.has(key)) {
      seen.add(key);
      out.push(f);
    }
  };

  for (const el of document.querySelectorAll("body *")) {
    if (!visible(el)) continue;
    const cs = getComputedStyle(el);
    const isText = cs.textOverflow === "ellipsis" || cs.whiteSpace === "nowrap" || /\btruncate\b|line-clamp/.test(el.getAttribute("class") ?? "");
    const hiddenY = cs.overflowY === "hidden" || cs.overflowY === "clip";
    const hiddenX = cs.overflowX === "hidden" || cs.overflowX === "clip";
    // Cortes de 1-8 px são arredondamento, borda ou sombra.
    if (hiddenY && !isText && el.scrollHeight > el.clientHeight + 8 && el.clientHeight > 0) {
      push({ kind: "clipY", selector: describe(el), text: (el as HTMLElement).innerText?.slice(0, 60).replace(/\s+/g, " ") ?? "", detail: `conteúdo ${el.scrollHeight}px numa caixa de ${el.clientHeight}px` });
    }
    // Campo de texto rola o próprio texto: não é corte.
    const isField = el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT";
    if (hiddenX && !isText && !isField && el.scrollWidth > el.clientWidth + 8 && el.clientWidth > 0) {
      push({ kind: "clipX", selector: describe(el), text: (el as HTMLElement).innerText?.slice(0, 60).replace(/\s+/g, " ") ?? "", detail: `conteúdo ${el.scrollWidth}px numa caixa de ${el.clientWidth}px` });
    }
    const text = ownText(el) || (el.tagName === "INPUT" || el.tagName === "BUTTON" ? (el as HTMLInputElement).placeholder || (el as HTMLElement).innerText || el.tagName : "");
    if (!text) continue;
    const r = el.getBoundingClientRect();
    if (r.bottom > vh + 2 && !scrollsTo(el, "y")) {
      push({ kind: "offscreenY", selector: describe(el), text: text.slice(0, 60), detail: `termina em y=${Math.round(r.bottom)} (janela ${vh})` });
    }
    if (r.right > vw + 2 && !scrollsTo(el, "x")) {
      push({ kind: "offscreenX", selector: describe(el), text: text.slice(0, 60), detail: `termina em x=${Math.round(r.right)} (janela ${vw})` });
    }
  }
  return out;
}
