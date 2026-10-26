/**
 * O que a barra de título mostra como versão.
 *
 * A release (CI) grava o número certo no `tauri.conf.json` antes de compilar,
 * então `getVersion()` diz a verdade. O build local de teste não passa por isso
 * e diria o número parado do `package.json` (2.0.0): por isso o Vite injeta
 * `__TEST_BUILD_LABEL__` — a última versão publicada + o commit, ex.
 * "v2.3.0-beta + test 13c3b4f" — e na release ele vem vazio (vite.config.ts).
 */
declare const __TEST_BUILD_LABEL__: string | undefined;

export const TEST_BUILD_LABEL: string =
  typeof __TEST_BUILD_LABEL__ === "string" ? __TEST_BUILD_LABEL__ : "";

/** O texto da versão no título, ou `null` enquanto não se sabe. */
export function titleVersion(version: string | null, testLabel: string): string | null {
  if (testLabel) return testLabel;
  return version ? `v${version}` : null;
}
