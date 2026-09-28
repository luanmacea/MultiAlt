import { describe, expect, it } from "vitest";
import { nextReleaseVersion, setCargoPackageVersion } from "./release-version.mjs";

// Tags que já existem no repositório: a série 4 herdada do número do projeto
// original. A série 0.x não pode enxergá-las.
const SERIES_4 = ["v4.0.1-beta", "v4.0.2-beta", "v4.0.3-beta", "v4.0.4-beta", "v4.0.5-beta"];

describe("nextReleaseVersion", () => {
  it("opens a new series with the package.json version as is", () => {
    expect(nextReleaseVersion({ current: "0.1.0", tags: SERIES_4, bump: "patch" })).toEqual({
      version: "0.1.0",
      previousTag: "",
    });
  });

  it("bumps the patch from the highest tag of the series", () => {
    const tags = [...SERIES_4, "v0.1.0-beta", "v0.1.1-beta"];
    expect(nextReleaseVersion({ current: "0.1.0", tags, bump: "patch" })).toEqual({
      version: "0.1.2",
      previousTag: "v0.1.1-beta",
    });
  });

  it("resets the patch on a minor bump", () => {
    expect(nextReleaseVersion({ current: "0.1.0", tags: ["v0.1.3-beta"], bump: "minor" })).toEqual({
      version: "0.2.0",
      previousTag: "v0.1.3-beta",
    });
  });

  it("releases a version raised by hand in package.json as is", () => {
    // 1.0.0: a primeira versão completamente corrigida, escolhida pelo dono.
    const tags = [...SERIES_4, "v0.1.0-beta", "v0.4.2-beta"];
    expect(nextReleaseVersion({ current: "1.0.0", tags, bump: "patch" })).toEqual({
      version: "1.0.0",
      previousTag: "",
    });
    expect(nextReleaseVersion({ current: "0.3.0", tags: ["v0.1.5-beta"], bump: "patch" })).toEqual({
      version: "0.3.0",
      previousTag: "v0.1.5-beta",
    });
  });

  it("never repeats a version whose tag already exists", () => {
    // O workflow não commita o número de volta: o package.json fica parado na
    // versão que abriu a série, e a próxima release tem que somar.
    expect(nextReleaseVersion({ current: "0.1.0", tags: ["v0.1.0-beta"], bump: "patch" })).toEqual({
      version: "0.1.1",
      previousTag: "v0.1.0-beta",
    });
  });

  it("keeps counting the series 4 the way it always did", () => {
    expect(nextReleaseVersion({ current: "4.0.0", tags: SERIES_4, bump: "patch" })).toEqual({
      version: "4.0.6",
      previousTag: "v4.0.5-beta",
    });
  });

  it("ignores tags that are not release versions", () => {
    const tags = ["vnext", "v0.1", "latest", "v0.1.2-beta"];
    expect(nextReleaseVersion({ current: "0.1.0", tags, bump: "patch" })).toEqual({
      version: "0.1.3",
      previousTag: "v0.1.2-beta",
    });
  });

  it("rejects an invalid package.json version", () => {
    expect(() => nextReleaseVersion({ current: "0.1", tags: [], bump: "patch" })).toThrow(/0\.1/);
  });
});

describe("setCargoPackageVersion", () => {
  const CARGO = [
    "[package]",
    'name = "roblox-account-manager"',
    'version = "0.1.0"',
    'edition = "2021"',
    "",
    "[dependencies]",
    'serde = { version = "1", features = ["derive"] }',
    "",
  ].join("\n");

  it("rewrites only the package version line", () => {
    const out = setCargoPackageVersion(CARGO, "0.1.1");
    expect(out).toContain('\nversion = "0.1.1"\n');
    expect(out).toContain('serde = { version = "1", features = ["derive"] }');
  });

  it("accepts the version the file already has", () => {
    // Primeira release de uma serie: o numero ja esta no Cargo.toml. Era aqui
    // que o prepare-release.mjs parava com "Could not update version".
    expect(setCargoPackageVersion(CARGO, "0.1.0")).toBe(CARGO);
  });

  it("fails when the file has no package version line", () => {
    expect(() => setCargoPackageVersion('[package]\nname = "x"\n', "0.1.0")).toThrow(/version/);
  });
});
