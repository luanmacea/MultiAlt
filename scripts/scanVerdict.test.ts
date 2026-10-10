import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  classifyVirusTotal,
  defenderResult,
  parseEdition,
  scanSummary,
  scanTargets,
  virusTotalExitCode,
  virusTotalResult,
} from "./scanVerdict";

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

/**
 * Política das duas edições (decisão do dono, 10/10/2026): a padrão não aceita
 * marcação nenhuma; a completa aceita **só** o Trapmine com rótulo de ML
 * (`*.ml.score`). Qualquer outro motor, ou o Defender, reprova as duas.
 */
describe("veredito do VirusTotal por edição", () => {
  const trapmine = { engine: "Trapmine", result: "malicious.moderate.ml.score", category: "malicious" };
  const outro = { engine: "Microsoft", result: "Trojan:Win32/Wacatac.B!ml", category: "malicious" };

  it("sem marcação é limpo nas duas edições", () => {
    expect(classifyVirusTotal([], "standard")).toBe("clean");
    expect(classifyVirusTotal([], "full")).toBe("clean");
  });

  it("na padrão qualquer motor reprova, até o Trapmine de ML", () => {
    expect(classifyVirusTotal([trapmine], "standard")).toBe("flagged");
    expect(classifyVirusTotal([outro], "standard")).toBe("flagged");
  });

  it("na completa só o Trapmine de ML sozinho é aceito", () => {
    expect(classifyVirusTotal([trapmine], "full")).toBe("accepted");
    expect(classifyVirusTotal([{ ...trapmine, engine: "trapmine", category: "suspicious" }], "full")).toBe("accepted");
    expect(classifyVirusTotal([trapmine, outro], "full")).toBe("flagged");
    expect(classifyVirusTotal([outro], "full")).toBe("flagged");
    // Trapmine com rótulo que não é o de ML (assinatura de verdade) reprova.
    expect(classifyVirusTotal([{ ...trapmine, result: "malicious.trojan" }], "full")).toBe("flagged");
    expect(classifyVirusTotal([{ ...trapmine, result: "malicious.ml.score.v2" }], "full")).toBe("flagged");
    expect(classifyVirusTotal([{ ...trapmine, result: null }], "full")).toBe("flagged");
    // Outro motor com rótulo de ML não ganha a exceção.
    expect(classifyVirusTotal([{ ...outro, result: "malicious.moderate.ml.score" }], "full")).toBe("flagged");
  });

  it("o código de saída do virustotal.ts leva o aceito até o scan.ts", () => {
    for (const r of ["clean", "flagged", "accepted", "inconclusive"] as const) {
      expect(virusTotalResult(virusTotalExitCode(r))).toBe(r);
    }
    expect(virusTotalExitCode("clean")).toBe(0);
    expect(virusTotalExitCode("flagged")).toBe(2);
  });

  it("lê a edição dos argumentos, padrão por omissão", () => {
    expect(parseEdition([])).toBe("standard");
    expect(parseEdition(["app.msi"])).toBe("standard");
    expect(parseEdition(["--edition", "full", "app.msi"])).toBe("full");
    expect(parseEdition(["--edition=full"])).toBe("full");
    expect(parseEdition(["--edition", "standard"])).toBe("standard");
    expect(() => parseEdition(["--edition", "completa"])).toThrow(/edição/);
    expect(() => parseEdition(["--edition"])).toThrow(/edição/);
  });

  it("o valor da edição não vira arquivo a escanear", () => {
    expect(scanTargets(["--edition", "full", "a.msi", "b.exe"])).toEqual(["a.msi", "b.exe"]);
    expect(scanTargets(["--edition=full", "a.msi"])).toEqual(["a.msi"]);
  });

  it("o resumo diz aceito na completa e sai 0; o Defender continua reprovando", () => {
    const aceito = scanSummary([{ file: "full.msi", defender: "clean", virustotal: "accepted" }]);
    expect(aceito.exitCode).toBe(0);
    expect(aceito.lines.join("\n")).toMatch(/VirusTotal: aceito \(edição completa: só o Trapmine de ML\)\s+full\.msi/);
    expect(aceito.lines.join("\n")).not.toMatch(/tudo limpo/);

    const defender = scanSummary([{ file: "full.msi", defender: "flagged", virustotal: "accepted" }]);
    expect(defender.exitCode).toBe(2);
  });
});
