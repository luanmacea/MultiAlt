import { describe, expect, it } from "vitest";
import {
  AFK_DEFAULT_POINT,
  clampAfkPercent,
  formatAfkPoint,
  readAfkPoint,
  readAfkSettingsPoint,
  writeAfkPoint,
} from "./afkClickPoint";

describe("ponto do clique do AFK mode", () => {
  it("trava a porcentagem em 0–100 e troca lixo pelo meio", () => {
    expect(clampAfkPercent(150)).toBe(100);
    expect(clampAfkPercent(-5)).toBe(0);
    expect(clampAfkPercent(Number.NaN)).toBe(50);
    expect(clampAfkPercent(37.5)).toBe(37.5);
  });

  /** O backend (`afk_point_from_fields`) só aceita ponto próprio com os dois números. */
  it("lê o ponto da conta só com os dois números", () => {
    expect(readAfkPoint({ AfkClickX: "52.5", AfkClickY: "71" })).toEqual({ x: 52.5, y: 71 });
    expect(readAfkPoint({ AfkClickX: "52.5" })).toBeNull();
    expect(readAfkPoint({ AfkClickX: "abc", AfkClickY: "1" })).toBeNull();
    expect(readAfkPoint(undefined)).toBeNull();
    expect(readAfkPoint({ AfkClickX: "150", AfkClickY: "10" })).toEqual({ x: 100, y: 10 });
  });

  it("grava e apaga o ponto da conta sem mexer nos outros campos", () => {
    const withPoint = writeAfkPoint({ Alias: "main" }, { x: 10, y: 20.5 });
    expect(withPoint).toEqual({ Alias: "main", AfkClickX: "10", AfkClickY: "20.5" });
    expect(writeAfkPoint(withPoint, null)).toEqual({ Alias: "main" });
    expect(writeAfkPoint(undefined, null)).toEqual({});
  });

  it("o ponto padrão sai do INI, e sem INI é o meio da janela", () => {
    expect(readAfkSettingsPoint({ ClickX: "25", ClickY: "80" })).toEqual({ x: 25, y: 80 });
    expect(readAfkSettingsPoint({})).toEqual(AFK_DEFAULT_POINT);
    expect(readAfkSettingsPoint(undefined)).toEqual({ x: 50, y: 50 });
    expect(readAfkSettingsPoint({ ClickX: "x", ClickY: "10" })).toEqual({ x: 50, y: 10 });
  });

  it("mostra o ponto como duas porcentagens", () => {
    expect(formatAfkPoint({ x: 52.5, y: 71 })).toBe("52.5% × 71%");
  });
});
