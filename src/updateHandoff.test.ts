import { describe, expect, it } from "vitest";
import {
  UPDATE_HANDOFF_KEY,
  UPDATE_HANDOFF_MAX_AGE_MS,
  consumeUpdateHandoff,
  writeUpdateHandoff,
} from "./updateHandoff";

function memoryStorage(): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    clear: () => data.clear(),
    getItem: (k) => data.get(k) ?? null,
    key: (i) => [...data.keys()][i] ?? null,
    removeItem: (k) => void data.delete(k),
    setItem: (k, v) => void data.set(k, String(v)),
  };
}

const NOW = 1_800_000_000_000;

describe("updateHandoff", () => {
  it("anuncia a versão nova quando o app volta já atualizado", () => {
    const storage = memoryStorage();
    writeUpdateHandoff(storage, "0.1.6", "0.1.7", NOW);
    expect(consumeUpdateHandoff(storage, "0.1.7", NOW + 15_000)).toEqual({
      kind: "updated",
      version: "0.1.7",
    });
  });

  it("avisa que não concluiu quando o app volta na versão antiga", () => {
    const storage = memoryStorage();
    writeUpdateHandoff(storage, "0.1.6", "0.1.7", NOW);
    expect(consumeUpdateHandoff(storage, "0.1.6", NOW + 60_000)).toEqual({
      kind: "failed",
      version: "0.1.7",
    });
  });

  it("só anuncia uma vez", () => {
    const storage = memoryStorage();
    writeUpdateHandoff(storage, "0.1.6", "0.1.7", NOW);
    consumeUpdateHandoff(storage, "0.1.7", NOW);
    expect(storage.getItem(UPDATE_HANDOFF_KEY)).toBeNull();
    expect(consumeUpdateHandoff(storage, "0.1.7", NOW)).toBeNull();
  });

  it("ignora uma anotação velha demais", () => {
    const storage = memoryStorage();
    writeUpdateHandoff(storage, "0.1.6", "0.1.7", NOW);
    expect(consumeUpdateHandoff(storage, "0.1.6", NOW + UPDATE_HANDOFF_MAX_AGE_MS + 1)).toBeNull();
    expect(storage.getItem(UPDATE_HANDOFF_KEY)).toBeNull();
  });

  it("ignora versão que não é nem a de origem nem a de destino", () => {
    const storage = memoryStorage();
    writeUpdateHandoff(storage, "0.1.6", "0.1.7", NOW);
    expect(consumeUpdateHandoff(storage, "0.1.9", NOW)).toBeNull();
  });

  it("aguenta lixo no armazenamento", () => {
    const storage = memoryStorage();
    storage.setItem(UPDATE_HANDOFF_KEY, "{not json");
    expect(consumeUpdateHandoff(storage, "0.1.7", NOW)).toBeNull();
    storage.setItem(UPDATE_HANDOFF_KEY, JSON.stringify({ from: 1, to: null }));
    expect(consumeUpdateHandoff(storage, "0.1.7", NOW)).toBeNull();
  });

  it("não quebra quando o armazenamento recusa", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {
        throw new Error("blocked");
      },
    } as unknown as Storage;
    expect(() => writeUpdateHandoff(broken, "0.1.6", "0.1.7", NOW)).not.toThrow();
    expect(consumeUpdateHandoff(broken, "0.1.7", NOW)).toBeNull();
  });
});
