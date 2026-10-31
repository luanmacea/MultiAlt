/**
 * Instaladores Linux da release: nomes por edição, cópia para a pasta de envio
 * e manifesto do updater. Usado pelo job `linux` do release-v4.yml e pelo
 * linux-build.yml (build de teste). Nada aqui toca nos arquivos do Windows.
 *
 * Nomes (a padrão segue o `MultiAlt-...`, a completa o `..._Full-...` do MSI):
 *   padrão    MultiAlt_<v>_Linux_amd64.AppImage | .deb  /  MultiAlt_<v>_Linux_x86_64.rpm
 *   completa  MultiAlt_<v>_Full-Linux_amd64.AppImage | .deb  /  MultiAlt_<v>_Full-Linux_x86_64.rpm
 *
 * Chave do manifesto: `linux-x86_64-<appimage|deb|rpm>`, a que o plugin do
 * updater procura pelo formato com que o app foi instalado.
 */
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const EDITIONS = ["standard", "full"];

export const LINUX_FORMATS = [
  { kind: "appimage", bundleDir: "appimage", ext: "AppImage", arch: "amd64" },
  { kind: "deb", bundleDir: "deb", ext: "deb", arch: "amd64" },
  { kind: "rpm", bundleDir: "rpm", ext: "rpm", arch: "x86_64" },
];

/** Nome do arquivo na release. A completa nunca é linkada fora dos anexos. */
export function linuxAssetName(version, edition, kind) {
  if (!EDITIONS.includes(edition)) throw new Error(`Unknown edition: ${edition}`);
  const format = LINUX_FORMATS.find((f) => f.kind === kind);
  if (!format) throw new Error(`Unknown Linux format: ${kind}`);
  const label = edition === "full" ? "Full-Linux" : "Linux";
  return `MultiAlt_${version}_${label}_${format.arch}.${format.ext}`;
}

/** A assinatura do updater sobe como `zz-<arquivo>.sig`, no fim da lista de anexos. */
export function linuxSignatureName(assetName) {
  return `zz-${assetName}.sig`;
}

/** Canal do manifesto: `<release>-linux` (padrão) ou `<release>-linux-nexus-ws` (completa). */
export function linuxManifestChannel(releaseChannel, edition) {
  if (!EDITIONS.includes(edition)) throw new Error(`Unknown edition: ${edition}`);
  return edition === "full" ? `${releaseChannel}-linux-nexus-ws` : `${releaseChannel}-linux`;
}

function onlyBundle(dir, ext) {
  const found = existsSync(dir) ? readdirSync(dir).filter((n) => n.endsWith(`.${ext}`)) : [];
  if (found.length !== 1) {
    throw new Error(`Expected exactly one .${ext} in ${dir}, found ${found.length}: ${found.join(", ")}`);
  }
  return path.join(dir, found[0]);
}

/**
 * Copia os pacotes que o `tauri build` deixou em `<bundleRoot>/<formato>/` para
 * `outDir` com o nome da release. `sign: true` exige e copia também a assinatura
 * do updater. Devolve o que foi copiado.
 */
export function stageLinuxAssets({ bundleRoot, outDir, version, edition, sign = true }) {
  mkdirSync(outDir, { recursive: true });
  const staged = [];
  for (const format of LINUX_FORMATS) {
    const source = onlyBundle(path.join(bundleRoot, format.bundleDir), format.ext);
    const name = linuxAssetName(version, edition, format.kind);
    copyFileSync(source, path.join(outDir, name));
    const entry = { kind: format.kind, name };
    if (sign) {
      const sigSource = `${source}.sig`;
      if (!existsSync(sigSource)) throw new Error(`Missing updater signature: ${sigSource}`);
      const sigName = linuxSignatureName(name);
      copyFileSync(sigSource, path.join(outDir, sigName));
      entry.sigName = sigName;
      entry.signature = readFileSync(sigSource, "utf8").trim();
    }
    staged.push(entry);
  }
  return staged;
}

/** Manifesto do updater só com chaves `linux-x86_64-*`. */
export function linuxManifest({ version, repo, tag, staged }) {
  const baseUrl = `https://github.com/${repo}/releases/download/${tag}`;
  const platforms = {};
  for (const item of staged) {
    if (!item.signature) throw new Error(`No signature for ${item.name}`);
    platforms[`linux-x86_64-${item.kind}`] = { signature: item.signature, url: `${baseUrl}/${item.name}` };
  }
  return { version, notes: "", pub_date: new Date().toISOString(), platforms };
}

function arg(argv, name) {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? argv[i + 1] : undefined;
}

function main(argv) {
  const [command] = argv;
  if (command !== "stage") {
    throw new Error(
      "Usage: linux-release-assets.mjs stage --edition <standard|full> --version <v> --bundle <dir> --out <dir> [--no-sign] [--manifest <file> --repo <owner/name> --tag <tag>]"
    );
  }
  const edition = arg(argv, "edition");
  const version = arg(argv, "version");
  const bundleRoot = arg(argv, "bundle");
  const outDir = arg(argv, "out");
  const sign = !argv.includes("--no-sign");
  if (!edition || !version || !bundleRoot || !outDir) {
    throw new Error("--edition, --version, --bundle and --out are required");
  }
  const staged = stageLinuxAssets({ bundleRoot, outDir, version, edition, sign });
  for (const item of staged) console.log(`${item.kind}: ${item.name}`);
  const manifestPath = arg(argv, "manifest");
  if (manifestPath) {
    const repo = arg(argv, "repo");
    const tag = arg(argv, "tag");
    if (!sign || !repo || !tag) throw new Error("--manifest needs signatures, --repo and --tag");
    writeFileSync(manifestPath, `${JSON.stringify(linuxManifest({ version, repo, tag, staged }), null, 2)}\n`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2));
}
