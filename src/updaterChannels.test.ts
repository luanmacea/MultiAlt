import { describe, expect, it } from "vitest";
import {
  getUpdaterSkipVersionKey,
  normalizeUpdaterFeatureChannel,
  normalizeUpdaterReleaseChannel,
} from "./updaterChannels";

describe("normalizeUpdaterReleaseChannel", () => {
  it("only recognizes 'stable', case-insensitively", () => {
    expect(normalizeUpdaterReleaseChannel("stable")).toBe("stable");
    expect(normalizeUpdaterReleaseChannel("STABLE")).toBe("stable");
  });

  it("defaults everything else to beta", () => {
    expect(normalizeUpdaterReleaseChannel("beta")).toBe("beta");
    expect(normalizeUpdaterReleaseChannel("nightly")).toBe("beta");
    expect(normalizeUpdaterReleaseChannel("")).toBe("beta");
    expect(normalizeUpdaterReleaseChannel(null)).toBe("beta");
    expect(normalizeUpdaterReleaseChannel(undefined)).toBe("beta");
    expect(normalizeUpdaterReleaseChannel(" stable ")).toBe("beta");
  });
});

describe("normalizeUpdaterFeatureChannel", () => {
  it("maps the nexus aliases", () => {
    expect(normalizeUpdaterFeatureChannel("nexus-ws")).toBe("nexus-ws");
    expect(normalizeUpdaterFeatureChannel("nexus")).toBe("nexus-ws");
    expect(normalizeUpdaterFeatureChannel("full")).toBe("nexus-ws");
    expect(normalizeUpdaterFeatureChannel("FULL")).toBe("nexus-ws");
  });

  it("defaults everything else to standard", () => {
    expect(normalizeUpdaterFeatureChannel("standard")).toBe("standard");
    expect(normalizeUpdaterFeatureChannel("whatever")).toBe("standard");
    expect(normalizeUpdaterFeatureChannel(null)).toBe("standard");
    expect(normalizeUpdaterFeatureChannel(undefined)).toBe("standard");
  });
});

describe("getUpdaterSkipVersionKey", () => {
  it("namespaces the skipped version per channel pair", () => {
    expect(getUpdaterSkipVersionKey("beta", "standard")).toBe("skipped-update-version:beta:standard");
    expect(getUpdaterSkipVersionKey("stable", "nexus-ws")).toBe(
      "skipped-update-version:stable:nexus-ws"
    );
    expect(getUpdaterSkipVersionKey("beta", "standard")).not.toBe(
      getUpdaterSkipVersionKey("beta", "nexus-ws")
    );
  });
});
