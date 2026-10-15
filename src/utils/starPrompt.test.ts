import { describe, expect, it } from "vitest";
import {
  STAR_PROMPT_MIN_DAYS,
  STAR_PROMPT_SNOOZE_DAYS,
  finishStarPrompt,
  localDay,
  parseStarPrompt,
  recordUseDay,
  shouldShowStarPrompt,
  snoozeStarPrompt,
} from "./starPrompt";

const NOW = new Date(2026, 9, 15, 12).getTime();
const DAY = 24 * 60 * 60 * 1000;

describe("star prompt", () => {
  it("stays hidden until the app was used on three different days", () => {
    let state = parseStarPrompt(null);
    state = recordUseDay(state, "2026-10-13");
    state = recordUseDay(state, "2026-10-13");
    state = recordUseDay(state, "2026-10-14");
    expect(shouldShowStarPrompt(state, NOW)).toBe(false);
    state = recordUseDay(state, "2026-10-15");
    expect(state.days).toHaveLength(STAR_PROMPT_MIN_DAYS);
    expect(shouldShowStarPrompt(state, NOW)).toBe(true);
  });

  it("'Not now' hides it for two weeks, then it comes back", () => {
    const ready = { days: ["a", "b", "c"] };
    const snoozed = snoozeStarPrompt(ready, NOW);
    expect(shouldShowStarPrompt(snoozed, NOW + DAY)).toBe(false);
    expect(shouldShowStarPrompt(snoozed, NOW + STAR_PROMPT_SNOOZE_DAYS * DAY)).toBe(true);
  });

  it("starring or dismissing ends it for good", () => {
    const done = finishStarPrompt({ days: ["a", "b", "c"] });
    expect(shouldShowStarPrompt(done, NOW + 365 * DAY)).toBe(false);
    expect(recordUseDay(done, "2026-12-01")).toBe(done);
  });

  it("garbage in storage starts over instead of breaking the sidebar", () => {
    expect(parseStarPrompt("{not json")).toEqual({ days: [] });
    expect(parseStarPrompt('{"days":"x","done":"yes"}')).toEqual({ days: [], snoozeUntil: undefined, done: undefined });
  });

  it("counts days in local time", () => {
    expect(localDay(new Date(2026, 0, 5, 23, 59))).toBe("2026-01-05");
  });
});
