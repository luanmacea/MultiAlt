import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";

/**
 * O portatil (`MultiAlt_<v>_x64_portable*.exe`) saiu da release em 03/10/2026:
 * o `.exe` solto leva 1/75 de um motor de ML no VirusTotal, o MSI sai 0/75.
 * Ele nao foi apagado do workflow — fica atras do interruptor
 * `PUBLISH_PORTABLE`, para o dono religar trocando uma palavra. Estes testes
 * garantem as duas metades: o padrao e "false", e nenhum passo publica o
 * portatil sem consultar o interruptor (senao "desligado" so desligaria metade).
 */
const WORKFLOW = path.resolve(__dirname, "..", "workflows", "release-v4.yml");
const yaml = readFileSync(WORKFLOW, "utf8").replace(/\r\n/g, "\n");

/** Passos do job, pelo `- name:` de cada um. */
function steps() {
  const lines = yaml.split("\n");
  const out = [];
  let current = null;
  for (const line of lines) {
    const m = /^ {6}- name: (.+)$/.exec(line);
    if (m) {
      current = { name: m[1].trim(), lines: [] };
      out.push(current);
    } else if (current) {
      current.lines.push(line);
    }
  }
  return out;
}

const GUARD_IF = "if: env.PUBLISH_PORTABLE == 'true'";

/**
 * Linhas que ficam dentro de um bloco `{ ... }` aberto por uma linha que casa
 * com `opener` (PowerShell `if ($env:PUBLISH_PORTABLE -eq "true") {` ou JS
 * `if (publishPortable) {`), pela indentacao da chave de fechamento.
 */
function linesOutsideGuard(lines, opener) {
  const outside = [];
  let closeIndent = null;
  for (const line of lines) {
    if (closeIndent !== null) {
      if (line.trim() === "}" || line.trim().startsWith("})")) {
        const indent = line.length - line.trimStart().length;
        if (indent === closeIndent) closeIndent = null;
      }
      continue;
    }
    if (opener.test(line)) {
      closeIndent = line.length - line.trimStart().length;
      continue;
    }
    outside.push(line);
  }
  return outside;
}

describe("interruptor do portatil no release-v4.yml", () => {
  it("existe uma vez so e vem desligado", () => {
    const defs = [...yaml.matchAll(/^\s+PUBLISH_PORTABLE:\s*"?(\w+)"?\s*$/gm)];
    expect(defs).toHaveLength(1);
    expect(defs[0][1]).toBe("false");
  });

  it("todo passo com portable no nome so roda com o interruptor ligado", () => {
    const portableSteps = steps().filter((s) => /portable/i.test(s.name));
    expect(portableSteps.length).toBeGreaterThan(0);
    for (const step of portableSteps) {
      expect(step.lines.some((l) => l.trim() === GUARD_IF), step.name).toBe(true);
    }
  });

  it("nenhum upload do portatil fica fora de um passo protegido", () => {
    for (const step of steps()) {
      const uploadsPortable = step.lines.some((l) => /gh release upload/.test(l) && /portable/i.test(l));
      if (!uploadsPortable) continue;
      expect(step.lines.some((l) => l.trim() === GUARD_IF), step.name).toBe(true);
    }
  });

  it("a copia do portatil para a pasta da release so acontece com o interruptor ligado", () => {
    const prepare = steps().find((s) => s.name === "Prepare full feature assets and manifest");
    expect(prepare).toBeDefined();
    const outside = linesOutsideGuard(prepare.lines, /if \(\$env:PUBLISH_PORTABLE -eq "true"\) \{\s*$/);
    const leaked = outside.filter((l) => /Copy-Item \$(standard|full)PortableSrc/.test(l) || /Portable binary not found/i.test(l));
    expect(leaked).toEqual([]);
    // E o bloco protegido existe de fato (senao o filtro acima passa vazio).
    expect(prepare.lines.some((l) => /Copy-Item \$standardPortableSrc/.test(l))).toBe(true);
  });

  it("o guia de download da release so cita o portatil com o interruptor ligado", () => {
    const finalize = steps().find((s) => s.name === "Finalize release notes");
    expect(finalize).toBeDefined();
    expect(finalize.lines.some((l) => /process\.env\.PUBLISH_PORTABLE === "true"/.test(l))).toBe(true);
    const outside = linesOutsideGuard(finalize.lines, /if \(publishPortable\) \{\s*$/);
    expect(outside.filter((l) => /portable/i.test(l.replace(/PUBLISH_PORTABLE|publishPortable/g, "")))).toEqual([]);
  });
});
