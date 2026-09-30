import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { defenderResult, scanSummary, virusTotalResult } from "./scanVerdict";

describe("veredito do bun run scan", () => {
  it("lê o código de saída de cada motor", () => {
    expect(defenderResult(0)).toBe("clean");
    expect(defenderResult(2)).toBe("flagged");
    // Script que nem rodou (erro de sintaxe, MpCmdRun ausente) não é "limpo".
    expect(defenderResult(1)).toBe("inconclusive");
    expect(virusTotalResult(0)).toBe("clean");
    expect(virusTotalResult(2)).toBe("flagged");
    expect(virusTotalResult(1)).toBe("inconclusive");
  });

  /**
   * Em 29/09/2026 o scan disse "tudo limpo nos dois motores" com o Defender
   * quebrado por erro de sintaxe e o setup NSIS marcado 1/75 no VirusTotal.
   */
  it("não diz 'tudo limpo' com motor que marcou ou que não rodou", () => {
    const marcou = scanSummary([
      { file: "setup.exe", defender: "clean", virustotal: "flagged" },
      { file: "app.msi", defender: "clean", virustotal: "clean" },
    ]);
    expect(marcou.exitCode).toBe(2);
    expect(marcou.lines.join("\n")).toMatch(/algum motor marcou/);
    expect(marcou.lines.join("\n")).toMatch(/VirusTotal: MARCOU\s+setup\.exe/);

    const naoRodou = scanSummary([{ file: "app.msi", defender: "inconclusive", virustotal: "clean" }]);
    expect(naoRodou.exitCode).toBe(1);
    expect(naoRodou.lines.join("\n")).toMatch(/inconclusivo/);
    expect(naoRodou.lines.join("\n")).not.toMatch(/tudo limpo/);

    const limpo = scanSummary([{ file: "app.msi", defender: "clean", virustotal: "clean" }]);
    expect(limpo.exitCode).toBe(0);
    expect(limpo.lines.join("\n")).toMatch(/tudo limpo nos dois motores/);
  });

  it("sem arquivo escaneado não há o que declarar limpo", () => {
    expect(scanSummary([]).exitCode).toBe(1);
  });

  /**
   * O Windows PowerShell 5.1 lê `.ps1` sem BOM na página de código do sistema:
   * o travessão em UTF-8 vira `â€”`, e o `”` do meio fecha a string — o script
   * inteiro deixa de compilar.
   */
  it("o script do Defender só tem ASCII", () => {
    const bytes = readFileSync(join(import.meta.dirname, "defender-scan.ps1"));
    const fora = [...bytes].findIndex((b) => b > 0x7f);
    expect(fora, `byte não ASCII na posição ${fora}`).toBe(-1);
  });
});
