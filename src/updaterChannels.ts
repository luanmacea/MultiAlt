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

/**
 * A edição para a qual o app atualiza: a que a pessoa **escolheu** (Settings ›
 * Updates ou o botão da edição completa: `UpdaterFeatureChannelChosen = true`),
 * ou, sem escolha, **a que está instalada** (`running`, do backend).
 *
 * Relato do dono (28/10/2026): instalou a completa e o app atualizou para a
 * padrão. O `UpdaterFeatureChannel` do INI vinha de uma instalação anterior da
 * padrão (a versão nova nunca reescreve chave que existe), e era seguido mesmo
 * sem ninguém ter escolhido.
 */
export function resolveUpdaterFeatureChannel(
  stored: string | null | undefined,
  chosen: string | null | undefined,
  running: UpdaterFeatureChannel
): UpdaterFeatureChannel {
  return chosen === "true" && String(stored ?? "").trim() ? normalizeUpdaterFeatureChannel(stored) : running;
}

export function getUpdaterSkipVersionKey(
  releaseChannel: UpdaterReleaseChannel,
  featureChannel: UpdaterFeatureChannel
): string {
  return `skipped-update-version:${releaseChannel}:${featureChannel}`;
}
