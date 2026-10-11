import { describe, expect, it } from "vitest";
import {
  MEMORY_LIMIT_PRESETS_MB,
  fieldsWithMemoryLimit,
  formatMemoryMb,
  formatMemoryReading,
  memoryLimitChoice,
  parseMemoryLimit,
  parseCustomMemoryLimit,
} from "./memoryLimit";

/**
 * Espelho do `parse_memory_limit`/`effective_memory_limit` de
 * commands/memory_ceiling.rs: o campo da conta vence o padrão; "0" é sem limite.
 */
describe("memoryLimit", () => {
  it("reads the account's own limit, off, or nothing (follows the default)", () => {
    expect(parseMemoryLimit("2048")).toBe(2048);
    expect(parseMemoryLimit("0")).toBe(0);
    expect(parseMemoryLimit("off")).toBe(0);
    expect(parseMemoryLimit("")).toBeNull();
    expect(parseMemoryLimit(undefined)).toBeNull();
    expect(parseMemoryLimit("abc")).toBeNull();
  });

  it("keeps a limit between 256 MB and 64 GB, like the backend", () => {
    expect(parseMemoryLimit("10")).toBe(256);
    expect(parseMemoryLimit("999999")).toBe(65536);
  });

  it("says where the limit comes from", () => {
    expect(memoryLimitChoice({ MemoryLimit: "1536" }, "2048")).toEqual({
      own: 1536,
      defaultMb: 2048,
      effectiveMb: 1536,
    });
    expect(memoryLimitChoice({}, "2048")).toEqual({ own: null, defaultMb: 2048, effectiveMb: 2048 });
    expect(memoryLimitChoice({ MemoryLimit: "0" }, "2048")).toEqual({ own: 0, defaultMb: 2048, effectiveMb: 0 });
    expect(memoryLimitChoice(undefined, undefined)).toEqual({ own: null, defaultMb: 0, effectiveMb: 0 });
  });

  it("writes the field, or removes it to follow the default, without touching the others", () => {
    expect(fieldsWithMemoryLimit({ Note: "x" }, 2048)).toEqual({ Note: "x", MemoryLimit: "2048" });
    expect(fieldsWithMemoryLimit({ Note: "x", MemoryLimit: "2048" }, 0)).toEqual({ Note: "x", MemoryLimit: "0" });
    expect(fieldsWithMemoryLimit({ Note: "x", MemoryLimit: "2048" }, null)).toEqual({ Note: "x" });
  });

  it("shows megabytes as GB from 1 GB on", () => {
    expect(formatMemoryMb(900)).toBe("900 MB");
    expect(formatMemoryMb(1024)).toBe("1 GB");
    expect(formatMemoryMb(1536)).toBe("1.5 GB");
    expect(formatMemoryMb(2500)).toBe("2.4 GB");
  });

  it("offers the usual sizes", () => {
    expect(MEMORY_LIMIT_PRESETS_MB).toEqual([1024, 1536, 2048, 3072, 4096]);
  });

  it("reads a custom value typed in MB or GB", () => {
    expect(parseCustomMemoryLimit("1800")).toBe(1800);
    expect(parseCustomMemoryLimit("2.5 GB")).toBe(2560);
    expect(parseCustomMemoryLimit("3gb")).toBe(3072);
    expect(parseCustomMemoryLimit("700 MB")).toBe(700);
    expect(parseCustomMemoryLimit("0")).toBeNull();
    expect(parseCustomMemoryLimit("lots")).toBeNull();
  });
});

describe("formatMemoryMb — decimal separator", () => {
  it("uses a comma in Portuguese, Spanish and German", () => {
    expect(formatMemoryMb(1250, "pt")).toBe("1,2 GB");
    expect(formatMemoryMb(1536, "es")).toBe("1,5 GB");
    expect(formatMemoryMb(1536, "de")).toBe("1,5 GB");
    expect(formatMemoryMb(1536, "en")).toBe("1.5 GB");
    expect(formatMemoryMb(2048, "pt")).toBe("2 GB");
    expect(formatMemoryMb(900, "pt")).toBe("900 MB");
  });
});

/**
 * A leitura do cliente vai em MB, como a coluna "Memória" do Gerenciador de
 * Tarefas: dá para comparar um com o outro, e uma mudança de dezenas de MB
 * aparece (em GB com uma casa, 4,0 a 4,05 GB eram todos "4 GB" — parecia
 * congelado, 11/10/2026).
 */
describe("formatMemoryReading", () => {
  it("shows whole megabytes with a thousands separator, like Task Manager", () => {
    expect(formatMemoryReading(1187.4)).toBe("1,187 MB");
    expect(formatMemoryReading(1187, "en")).toBe("1,187 MB");
    expect(formatMemoryReading(1187, "pt-BR")).toBe("1.187 MB");
    expect(formatMemoryReading(1187, "es")).toBe("1.187 MB");
    expect(formatMemoryReading(880, "pt")).toBe("880 MB");
    expect(formatMemoryReading(12345, "en")).toBe("12,345 MB");
  });

  it("a small change shows up", () => {
    expect(formatMemoryReading(4096)).not.toBe(formatMemoryReading(4130));
  });
});
