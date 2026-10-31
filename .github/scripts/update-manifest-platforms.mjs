/**
 * O MSI e o unico instalador publicado desde 03/10/2026 (o setup NSIS saiu:
 * levava marcacao heuristica no VirusTotal). O manifesto do updater, entao, so
 * carrega a chave `windows-x86_64-msi`.
 *
 * - O tauri-action escreve tambem a chave generica `windows-x86_64` apontando
 *   para o MSI. Ela e a que o updater usa quando o app nao sabe o proprio
 *   formato — o portatil, ou um NSIS antigo — e mandar o MSI para esses
 *   instalaria uma **segunda copia** ao lado da atual. Sai.
 * - Quem instalou pelo setup NSIS deixa de receber atualizacao (decisao do dono,
 *   03/10/2026): o README orienta a instalar o MSI uma vez.
 */

/** Nome fixo do MSI na release: o botao de download do README aponta para ele. */
export const STABLE_MSI_NAME = "MultiAlt-Setup.msi";

/**
 * Um setup só na release (pedido do dono, 03/10/2026): o `MultiAlt_<v>_x64_en-US.msi`
 * que o tauri-action sobe é byte a byte o `MultiAlt-Setup.msi` (o "en-US" é só o
 * idioma da janela do instalador). A cópia com versão sai da release, então o
 * updater baixa o de nome fixo da mesma tag; a assinatura é do conteúdo e vale
 * igual. O MSI da edição completa (`MultiAlt_<v>_Full-Setup.msi`) é outro arquivo e fica.
 */
export function toStableMsiUrl(url) {
  return url.replace(/\/MultiAlt_[^/]*_x64_en-US\.msi$/, `/${STABLE_MSI_NAME}`);
}

export function msiOnlyPlatforms(platforms) {
  // Com um formato so, o tauri-action pode escrever apenas a chave generica;
  // se ela aponta para um .msi, e a do MSI.
  const msi = platforms?.["windows-x86_64-msi"] ?? platforms?.["windows-x86_64"];
  if (!msi) {
    throw new Error("update manifest has no windows-x86_64-msi entry");
  }
  if (!String(msi.url || "").toLowerCase().endsWith(".msi")) {
    throw new Error(`windows-x86_64-msi must point at an .msi, got ${msi.url}`);
  }
  return { "windows-x86_64-msi": { ...msi, url: toStableMsiUrl(String(msi.url)) } };
}

/**
 * Manifestos do Linux (`<canal>-linux`, `<canal>-linux-nexus-ws`): so chaves
 * `linux-x86_64-<appimage|deb|rpm>`, cada uma apontando para o arquivo do
 * proprio formato. O AppImage e obrigatorio (e o que o README do Linux
 * recomenda); deb e rpm entram quando existem. Nunca mistura chaves de Windows:
 * os manifestos dos dois sistemas sao arquivos diferentes.
 */
const LINUX_EXTENSIONS = { appimage: ".appimage", deb: ".deb", rpm: ".rpm" };

export function linuxOnlyPlatforms(platforms) {
  const out = {};
  for (const [kind, extension] of Object.entries(LINUX_EXTENSIONS)) {
    const entry = platforms?.[`linux-x86_64-${kind}`];
    if (!entry) continue;
    if (!String(entry.url || "").toLowerCase().endsWith(extension)) {
      throw new Error(`linux-x86_64-${kind} must point at a ${extension} file, got ${entry.url}`);
    }
    out[`linux-x86_64-${kind}`] = entry;
  }
  if (!out["linux-x86_64-appimage"]) {
    throw new Error("update manifest has no linux-x86_64-appimage entry");
  }
  return out;
}

/** Escolhe as plataformas do manifesto pela familia (`RELEASE_PLATFORM_FAMILY`); sem ela, e o Windows de sempre. */
export function platformsForFamily(family, platforms) {
  return String(family || "").toLowerCase() === "linux" ? linuxOnlyPlatforms(platforms) : msiOnlyPlatforms(platforms);
}
