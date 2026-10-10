import { describe, expect, it } from "vitest";
import {
  favoriteTargets,
  formatNextRun,
  matchingTarget,
  newPresetDraft,
  presetDayToJsDay,
  presetProblem,
  vipJobFromLink,
} from "./presets";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, key) => String(options?.[key] ?? ""));
const weekday = (jsDay: number) => ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][jsDay];

describe("presets — destino a partir dos favoritos", () => {
  it("each favorite becomes its public server plus one entry per saved VIP", () => {
    const options = favoriteTargets([
      {
        placeId: 10,
        name: "Blox Fruits",
        iconUrl: null,
        addedAt: 1,
        vipServers: [
          { id: "a", name: "Squad", link: "https://www.roblox.com/games/10/x?privateServerLinkCode=ABC123" },
          { id: "b", name: "Empty", link: "  " },
        ],
      },
    ]);
    expect(options).toEqual([
      { key: "10:public", placeId: 10, jobId: "", gameName: "Blox Fruits", vipName: null },
      // O link vira `vip:<código>`, a forma que a fila de várias contas entende.
      { key: "10:a", placeId: 10, jobId: "vip:ABC123", gameName: "Blox Fruits", vipName: "Squad" },
    ]);
    expect(matchingTarget(options, 10, "vip:ABC123")?.key).toBe("10:a");
    expect(matchingTarget(options, 10, "some-job")).toBeNull();
  });

  it("a vip: code stays as it is and a plain Job ID is not touched", () => {
    expect(vipJobFromLink("vip:XYZ")).toBe("vip:XYZ");
    expect(vipJobFromLink(" 1b2c-job ")).toBe("1b2c-job");
  });
});

describe("presets — rascunho e validação", () => {
  it("a new draft keeps each account once and starts without a schedule", () => {
    const draft = newPresetDraft([3, 1, 3], 42, " job ");
    expect(draft.userIds).toEqual([3, 1]);
    expect(draft.placeId).toBe(42);
    expect(draft.jobId).toBe("job");
    expect(draft.schedule).toBeNull();
    expect(draft.id).toBe("");
  });

  it("says what is missing, in the order the form reads", () => {
    const ok = { ...newPresetDraft([1], 42), name: "Farm" };
    expect(presetProblem(ok)).toBeNull();
    expect(presetProblem({ ...ok, name: " " })).toBe("Give the preset a name.");
    expect(presetProblem({ ...ok, userIds: [] })).toBe("Pick at least one account.");
    expect(presetProblem({ ...ok, placeId: 0 })).toBe("Pick a game (Place ID).");
    expect(
      presetProblem({
        ...ok,
        schedule: { openEnabled: true, openAt: "08:00", days: [], closeEnabled: false, closeAt: "18:00" },
      })
    ).toBe("Pick at least one day to open.");
  });
});

describe("presets — próximo horário", () => {
  const monday = new Date(2026, 9, 12, 7, 0).getTime();

  it("today, tomorrow, or the weekday name", () => {
    expect(formatNextRun(new Date(2026, 9, 12, 8, 5).getTime(), monday, t, weekday)).toBe("Today 08:05");
    expect(formatNextRun(new Date(2026, 9, 13, 8, 0).getTime(), monday, t, weekday)).toBe("Tomorrow 08:00");
    expect(formatNextRun(new Date(2026, 9, 16, 18, 30).getTime(), monday, t, weekday)).toBe("Fri 18:30");
  });

  it("the backend's day (0 = Monday) maps to the JS day (0 = Sunday)", () => {
    expect(presetDayToJsDay(0)).toBe(1);
    expect(presetDayToJsDay(6)).toBe(0);
  });
});
