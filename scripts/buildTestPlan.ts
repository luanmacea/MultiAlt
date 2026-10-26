/**
 * O que o `bun run build:test` compila: o perfil `test-exe` (rápido, sem LTO)
 * de uma ou das duas edições, e o nome do executável na pasta de testes do
 * dono. Separado do script para ser testado sem compilar nada.
 */
export type TestEdition = "standard" | "full";

export interface TestBuildStep {
  edition: TestEdition;
  /** Argumentos do `bun run tauri build`. */
  tauriArgs: string[];
  /** Nome do executável em `MultiAlt-testes`. */
  fileName: string;
}

export const TEST_PROFILE = "test-exe";

export function testBuildPlan(args: string[]): TestBuildStep[] {
  const at = args.indexOf("--edition");
  const value = at >= 0 ? args[at + 1] : "both";
  const editions: TestEdition[] =
    value === "standard" || value === "default"
      ? ["standard"]
      : value === "full"
        ? ["full"]
        : value === "both"
          ? ["standard", "full"]
          : (() => {
              throw new Error(`--edition: use standard, full ou both (veio "${value}")`);
            })();
  return editions.map((edition) => ({
    edition,
    tauriArgs: [
      "tauri",
      "build",
      "--no-bundle",
      "--",
      "--profile",
      TEST_PROFILE,
      ...(edition === "standard" ? ["--no-default-features", "--features", "standard"] : []),
    ],
    fileName: edition === "standard" ? "MultiAlt-test-default.exe" : "MultiAlt-test-full.exe",
  }));
}
