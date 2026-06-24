export type UpdaterReleaseChannel = "beta" | "stable";
export type UpdaterFeatureChannel = "standard" | "nexus-ws";

export function normalizeUpdaterReleaseChannel(value: string | null | undefined): UpdaterReleaseChannel {
  return String(value || "").toLowerCase() === "stable" ? "stable" : "beta";
}

export function normalizeUpdaterFeatureChannel(value: string | null | undefined): UpdaterFeatureChannel {
  const normalized = String(value || "").toLowerCase();
  if (normalized === "nexus-ws" || normalized === "nexus" || normalized === "full") {
    return "nexus-ws";
  }
  return "standard";
}

export function getUpdaterSkipVersionKey(
  releaseChannel: UpdaterReleaseChannel,
  featureChannel: UpdaterFeatureChannel
): string {
  return `skipped-update-version:${releaseChannel}:${featureChannel}`;
}
