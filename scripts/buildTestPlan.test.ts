import { describe, expect, it } from "vitest";
import { testBuildPlan } from "./buildTestPlan";

describe("testBuildPlan", () => {
  it("builds both editions with the fast profile by default", () => {
    const plan = testBuildPlan([]);
    expect(plan.map((s) => s.edition)).toEqual(["standard", "full"]);
    for (const step of plan) {
      expect(step.tauriArgs).toContain("--no-bundle");
      expect(step.tauriArgs.slice(step.tauriArgs.indexOf("--profile"))[1]).toBe("test-exe");
      // O release (o que é escaneado e publicado) nunca sai daqui.
      expect(step.tauriArgs).not.toContain("--release");
    }
  });

  it("builds the standard edition with only the standard features", () => {
    const [step] = testBuildPlan(["--edition", "standard"]);
    expect(step.tauriArgs).toEqual(expect.arrayContaining(["--no-default-features", "--features", "standard"]));
    expect(step.fileName).toBe("MultiAlt-test-default.exe");
  });

  it("builds the full edition with the default features", () => {
    const [step] = testBuildPlan(["--edition", "full"]);
    expect(step.tauriArgs).not.toContain("--no-default-features");
    expect(step.fileName).toBe("MultiAlt-test-full.exe");
  });

  it("rejects an unknown edition", () => {
    expect(() => testBuildPlan(["--edition", "lite"])).toThrow(/--edition/);
  });
});
