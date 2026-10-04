import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
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

/**
 * O texto da release é lido por gente que não é técnica (pedido do dono,
 * 03/10/2026): abre com o botão do instalador, sem commit, canal ou conversa
 * sobre o setup antigo; os outros arquivos ficam num bloco recolhido. Os
 * títulos "## What's Changed" e "## Contributors" ficam porque o UpdateDialog
 * do app procura os dois.
 */
describe("texto da release", () => {
  const finalize = () => steps().find((s) => s.name === "Finalize release notes").lines.join("\n");

  it("abre com o download do MSI", () => {
    const body = finalize();
    const download = body.indexOf('"## Download"');
    expect(download).toBeGreaterThan(-1);
    expect(body).toMatch(/releases\/download\/\$\{tag\}\/MultiAlt-Setup\.msi/);
    // Os outros arquivos entram depois do download, recolhidos.
    expect(body.indexOf("...otherFiles")).toBeGreaterThan(download);
    expect(body).toMatch(/"<details>"/);
  });

  it("não mostra detalhe técnico nem o setup antigo no topo", () => {
    const body = finalize();
    expect(body).not.toMatch(/Release commit|Channel: |App version:|old setup/);
  });

  it("mantém os títulos que o app usa para montar as notas da atualização", () => {
    const body = finalize();
    expect(body).toMatch(/## What's Changed/);
    expect(body).toMatch(/## Contributors/);
  });
});

/**
 * "What's Changed" em linguagem simples: vem da seção "## What's new" do PR da
 * release; a lista automática do GitHub (títulos de PR) fica recolhida. Sem a
 * seção, a lista automática continua sendo o texto (nada sai vazio).
 */
describe("o que mudou, em linguagem simples", () => {
  const finalize = () => steps().find((s) => s.name === "Finalize release notes").lines.join("\n");

  it("lê a seção What's new do PR da release", () => {
    const body = finalize();
    expect(body).toMatch(/listPullRequestsAssociatedWithCommit/);
    expect(body).toMatch(/What's new/);
  });

  it("guarda a lista técnica recolhida e cai nela quando o PR não tem a seção", () => {
    const body = finalize();
    expect(body).toMatch(/<summary>Technical details<\/summary>/);
    expect(body).toMatch(/: generatedBody \|\| "## What's Changed/);
    expect(body).toMatch(/^\s+changesBlock,$/m);
  });
});

describe("um setup só na release", () => {
  const body = (name) => steps().find((s) => s.name === name)?.lines.join("\n") ?? "";
  const names = () => steps().map((s) => s.name);

  it("sobe o MultiAlt-Setup.msi antes de publicar o manifesto do updater", () => {
    const upload = names().findIndex((n) => /Upload stable-named MSI/.test(n));
    const manifest = names().indexOf("Generate and push standard update manifest");
    expect(upload).toBeGreaterThan(-1);
    expect(upload).toBeLessThan(manifest);
  });

  it("tira da release a cópia com a versão no nome (o mesmo arquivo)", () => {
    expect(body("Clean up release assets")).toMatch(/MultiAlt_\*_x64_en-US\.msi|x64_en-US\\.msi/);
  });

  it("o texto da release não cita mais a cópia com a versão no nome", () => {
    expect(body("Finalize release notes")).not.toMatch(/the same installer, with the version in the name/);
  });
});

/**
 * A 1.0.0 abriu a série 1.x sem tag anterior, e o bloco de contribuidores saiu
 * "No contributor metadata found" — também na janela de atualização do app. O
 * dono do repositório entra sempre (pedido do dono, 03/10/2026).
 */
describe("contribuidores da release", () => {
  it("sempre incluem o dono do repositório, mesmo sem tag anterior", () => {
    const finalize = steps().find((s) => s.name === "Finalize release notes").lines.join("\n");
    const add = finalize.indexOf("contributorSet.add(owner)");
    expect(add).toBeGreaterThan(-1);
    expect(add).toBeLessThan(finalize.indexOf("if (previousTag) {\n              try {\n                const compare"));
  });
});

/**
 * Tipo da release (correção, novidades, atualização geral — regra em
 * release-kind.mjs). O tipo decide o bump, então sai do passo "Resolve bump
 * and channel", antes do "Prepare version and release metadata"; e os dois
 * textos da release (o inicial, que vai no `notes` do manifesto do updater, e
 * o final) abrem com a marca e o selo.
 */
describe("tipo da release", () => {
  const step = (name) => steps().find((s) => s.name === name)?.lines.join("\n") ?? "";
  const names = () => steps().map((s) => s.name);

  it("o passo que resolve o bump decide o tipo pela regra testada, com o texto do PR", () => {
    const resolve = step("Resolve bump and channel");
    expect(resolve).toMatch(/listPullRequestsAssociatedWithCommit/);
    expect(resolve).toMatch(/release-kind\.mjs/);
    // Windows: import() de caminho absoluto só funciona como file:// URL.
    expect(resolve).toMatch(/pathToFileURL/);
    expect(resolve).toMatch(/resolveReleaseControls\(/);
    expect(resolve).toMatch(/prBody: .*pr\?*\.body/);
    expect(resolve).toMatch(/core\.setOutput\("kind"/);
    expect(resolve).toMatch(/core\.setOutput\("bump"/);
    expect(names().indexOf("Resolve bump and channel")).toBeLessThan(names().indexOf("Prepare version and release metadata"));
  });

  it("o tipo chega ao prepare-release e ao texto final", () => {
    expect(step("Prepare version and release metadata")).toMatch(/RELEASE_KIND: \$\{\{ steps\.controls\.outputs\.kind \}\}/);
    expect(step("Finalize release notes")).toMatch(/RELEASE_KIND: \$\{\{ steps\.controls\.outputs\.kind \}\}/);
  });

  it("o texto final abre com a marca e o selo, antes do download", () => {
    const finalize = step("Finalize release notes");
    expect(finalize).toMatch(/releaseKindHeader\(/);
    expect(finalize).toMatch(/pathToFileURL/);
    const final = /const finalBody = \[\s*([\s\S]*?)\]\.join/.exec(finalize);
    expect(final).not.toBeNull();
    const parts = final[1].split(",").map((p) => p.trim()).filter(Boolean);
    expect(parts[0]).toBe("kindHeader");
    expect(parts.indexOf("topNote")).toBeGreaterThan(0);
  });

  it("o bump do workflow_dispatch deixa o tipo decidir por padrão", () => {
    const dispatch = /bump:\n\s+description:.*\n\s+required: false\n\s+default: "(\w+)"/.exec(yaml);
    expect(dispatch?.[1]).toBe("auto");
    expect(yaml).toMatch(/- auto\n\s+- patch\n\s+- minor/);
  });

  it("o título da release não muda (o selo fica no texto)", () => {
    const prepare = readFileSync(path.resolve(__dirname, "prepare-release.mjs"), "utf8");
    expect(prepare).toMatch(/const releaseTitle = `MultiAlt \(Roblox Account Manager\) \$\{tag\}`;/);
  });
});

/**
 * O prepare-release.mjs de verdade, numa pasta temporária com um repositório
 * git de mentira: o texto inicial da release (o que o manifesto do updater
 * leva como `notes`) abre com a marca e o selo do tipo.
 */
describe("prepare-release.mjs", () => {
  function runPrepare(env) {
    const dir = mkdtempSync(path.join(tmpdir(), "multialt-prepare-"));
    try {
      mkdirSync(path.join(dir, "src-tauri"));
      writeFileSync(path.join(dir, "package.json"), JSON.stringify({ name: "x", version: "1.0.0" }));
      writeFileSync(path.join(dir, "src-tauri", "tauri.conf.json"), JSON.stringify({ version: "1.0.0" }));
      writeFileSync(path.join(dir, "src-tauri", "Cargo.toml"), '[package]\nname = "x"\nversion = "1.0.0"\n');
      const git = (...args) => execFileSync("git", args, { cwd: dir, stdio: "ignore" });
      git("init", "-q");
      git("-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "x");
      git("tag", "v1.0.0");
      const output = path.join(dir, "out.txt");
      writeFileSync(output, "");
      execFileSync(process.execPath, [path.resolve(__dirname, "prepare-release.mjs")], {
        cwd: dir,
        stdio: "pipe",
        env: { ...process.env, GITHUB_OUTPUT: output, GITHUB_REPOSITORY: "o/r", GITHUB_SHA: "abcdef1234", ...env },
      });
      return readFileSync(output, "utf8").replace(/\r\n/g, "\n");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }

  const bodyOf = (out) => /release_body<<EOF\n([\s\S]*?)\nEOF/.exec(out)?.[1] ?? "";

  it("correção: patch e o selo Hotfix na primeira linha", () => {
    const out = runPrepare({ RELEASE_BUMP: "patch", RELEASE_KIND: "fix" });
    expect(out).toMatch(/^version=1\.0\.1-beta$/m);
    expect(out).toMatch(/^kind=fix$/m);
    expect(bodyOf(out).startsWith("<!-- release-kind: fix -->\n**🩹 Hotfix**\n\n")).toBe(true);
  });

  it("novidades: o selo New features", () => {
    const out = runPrepare({ RELEASE_BUMP: "minor", RELEASE_KIND: "feature" });
    expect(out).toMatch(/^version=1\.1\.0-beta$/m);
    expect(bodyOf(out).startsWith("<!-- release-kind: feature -->\n**✨ New features**")).toBe(true);
  });

  it("recusa tipo desconhecido", () => {
    expect(() => runPrepare({ RELEASE_BUMP: "patch", RELEASE_KIND: "major" })).toThrow();
  });
});
