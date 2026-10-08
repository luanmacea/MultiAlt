import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";

/**
 * A `main` exige os checks `frontend` e `verify` (regra do repositorio desde
 * 08/10/2026). Um PR que nao disparasse o CI ficaria travado esperando check
 * que nunca chega — por isso o `pull_request` roda sempre, e o `guard` decide
 * pelos arquivos do PR (os mesmos caminhos do filtro do `push`).
 */
const yaml = readFileSync(path.resolve(__dirname, "..", "workflows", "ci.yml"), "utf8").replace(/\r\n/g, "\n");

function triggerBlock(name) {
  const start = yaml.indexOf(`  ${name}:\n`);
  const rest = yaml.slice(start + 1);
  const end = rest.search(/\n  [a-z_]+:\n|\n[a-z]/);
  return rest.slice(0, end);
}

describe("ci.yml e os checks obrigatorios da main", () => {
  it("o pull_request roda sem filtro de caminho", () => {
    expect(triggerBlock("pull_request")).not.toContain("paths:");
  });

  it("o guard olha os arquivos do PR e pula os jobs quando nao ha codigo", () => {
    expect(yaml).toContain("github.rest.pulls.listFiles");
    expect(yaml).toMatch(/core\.setOutput\('run', code \? 'true' : 'false'\)/);
  });

  it("os caminhos de codigo do guard sao os mesmos do filtro do push", () => {
    const push = triggerBlock("push");
    const pushPaths = [...push.matchAll(/- "([^"]+)"/g)].map((m) => m[1]);
    const line = yaml.split("\n").find((l) => l.includes("const CODE_PATHS = "));
    // eslint-disable-next-line no-new-func
    const codePaths = new Function(`${line.trim()} return CODE_PATHS;`)();
    for (const p of pushPaths) {
      const sample = p.replace("**", "x/y.ts");
      expect(codePaths.some((re) => re.test(sample)), p).toBe(true);
    }
    expect(codePaths.some((re) => re.test("README.md"))).toBe(false);
    expect(codePaths.some((re) => re.test(".github/ISSUE_TEMPLATE/bug_report.yml"))).toBe(false);
  });

  it("os jobs que a main exige dependem do guard", () => {
    for (const job of ["frontend", "verify"]) {
      const block = yaml.slice(yaml.indexOf(`  ${job}:\n`));
      expect(block).toMatch(/needs: guard\n\s+if: needs\.guard\.outputs\.run == 'true'/);
    }
  });
});
