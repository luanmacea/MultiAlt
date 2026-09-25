import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { WINDOWS_FASTFLAG_ALLOWLIST } from "./OptimizationTab";

/**
 * A allowlist e espelhada em TypeScript (ver o comentario em `OptimizationTab`
 * que justifica a escolha). Este teste e o que paga o preco do espelho: le o
 * `.rs` e compara com a constante TS, entao qualquer divergencia quebra a
 * suite em vez de virar fast flag descartado em silencio no launch.
 */
// `import.meta.url` nao e um `file:` sob o ambiente do vitest; o cwd da run e
// a raiz do repositorio.
const RUST_SOURCE = resolve(process.cwd(), "src-tauri/src/platform/windows/optimization.rs");

function rustAllowlist(): string[] {
  const source = readFileSync(RUST_SOURCE, "utf8");
  const block = source.match(
    /WINDOWS_FASTFLAG_ALLOWLIST\s*:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\]\s*;/
  );
  if (!block) throw new Error(`WINDOWS_FASTFLAG_ALLOWLIST nao encontrada em ${RUST_SOURCE}`);
  return [...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

describe("WINDOWS_FASTFLAG_ALLOWLIST", () => {
  it("matches the Rust allowlist the backend validates against", () => {
    expect(WINDOWS_FASTFLAG_ALLOWLIST).toEqual(rustAllowlist());
  });

  it("is not empty (guards against a regex that stopped matching)", () => {
    expect(rustAllowlist().length).toBeGreaterThan(0);
  });
});
