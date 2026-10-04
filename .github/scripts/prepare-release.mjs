import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { nextReleaseVersion, setCargoPackageVersion } from "./release-version.mjs";
import { RELEASE_KINDS, releaseKindHeader } from "./release-kind.mjs";

const bump = (process.env.RELEASE_BUMP || "patch").toLowerCase();
const channel = (process.env.RELEASE_CHANNEL || "beta").toLowerCase();
// Tipo da release (correcao, novidades, geral): decidido no passo "Resolve
// bump and channel" do release-v4.yml (regra em release-kind.mjs).
const kind = (process.env.RELEASE_KIND || "mixed").toLowerCase();
const repository = process.env.GITHUB_REPOSITORY || "";
const sha = process.env.GITHUB_SHA || "";

if (!["patch", "minor"].includes(bump)) {
  throw new Error(`Unsupported RELEASE_BUMP value: ${bump}`);
}

if (!RELEASE_KINDS.includes(kind)) {
  throw new Error(`Unsupported RELEASE_KIND value: ${kind}`);
}

if (!["beta", "stable"].includes(channel)) {
  throw new Error(`Unsupported RELEASE_CHANNEL value: ${channel}`);
}

if (!repository) {
  throw new Error("Missing GITHUB_REPOSITORY");
}

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = resolveRepoRoot([
  process.cwd(),
  path.resolve(scriptDir, "../..")
]);

const packagePath = path.join(repoRoot, "package.json");
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");
const cargoPath = path.join(repoRoot, "src-tauri", "Cargo.toml");

const missingPaths = [packagePath, tauriConfigPath, cargoPath].filter((item) => !fs.existsSync(item));
if (missingPaths.length > 0) {
  const missingRel = missingPaths.map((item) => path.relative(repoRoot, item)).join(", ");
  throw new Error(
    `Missing required release files: ${missingRel}. Ensure v4 rewrite files are committed on the target branch before running release.`
  );
}

const packageJson = JSON.parse(fs.readFileSync(packagePath, "utf8"));
const tauriConfig = JSON.parse(fs.readFileSync(tauriConfigPath, "utf8"));
const cargoToml = fs.readFileSync(cargoPath, "utf8");

const rawTags = execSync('git tag --list "v*"', { encoding: "utf8" })
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter(Boolean);

// Serie e numero da release: regra em release-version.mjs (suite `release`).
const { version: coreVersion, previousTag } = nextReleaseVersion({
  current: packageJson.version,
  tags: rawTags,
  bump
});
const fullVersion = channel === "beta" ? `${coreVersion}-beta` : coreVersion;
const appVersion = coreVersion;
const tag = `v${fullVersion}`;
const releaseTitle = `MultiAlt (Roblox Account Manager) ${tag}`;
const updaterEndpoint = `https://raw.githubusercontent.com/${repository}/update-manifests/${channel}/latest.json`;

packageJson.version = appVersion;

tauriConfig.version = appVersion;
tauriConfig.plugins ??= {};
tauriConfig.plugins.updater ??= {};
tauriConfig.plugins.updater.endpoints = [updaterEndpoint];

const updatedCargoToml = setCargoPackageVersion(cargoToml, appVersion);

fs.writeFileSync(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`, "utf8");
fs.writeFileSync(tauriConfigPath, `${JSON.stringify(tauriConfig, null, 2)}\n`, "utf8");
fs.writeFileSync(cargoPath, updatedCargoToml, "utf8");

const shortSha = sha ? sha.slice(0, 7) : "unknown";
const channelLabel = channel === "beta" ? "Beta" : "Stable";
// Primeira linha: a marca do tipo e o selo. Este texto e o `notes` do
// manifesto do updater (o manifesto sai antes do "Finalize release notes"),
// e o app le o tipo pela marca (src/releaseNotes.ts).
const releaseBody = [
  releaseKindHeader(kind),
  "",
  channel === "beta" ? "> [!WARNING]" : "> [!NOTE]",
  channel === "beta"
    ? "This is a beta release. Missing features, bugs, and crashes are possible. Run at your own risk."
    : "This is a stable release intended for general use.",
  "",
  `Channel: ${channelLabel}`,
  `Release commit: ${shortSha}`,
  `App version: ${appVersion}`
].join("\n");

setOutput("version", fullVersion);
setOutput("kind", kind);
setOutput("app_version", appVersion);
setOutput("tag", tag);
setOutput("release_title", releaseTitle);
setMultilineOutput("release_body", releaseBody);
setOutput("updater_endpoint", updaterEndpoint);
setOutput("previous_tag", previousTag);

function setOutput(name, value) {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) {
    return;
  }

  fs.appendFileSync(outputPath, `${name}=${value}\n`, "utf8");
}

function setMultilineOutput(name, value) {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) {
    return;
  }

  fs.appendFileSync(outputPath, `${name}<<EOF\n${value}\nEOF\n`, "utf8");
}

function resolveRepoRoot(candidates) {
  for (const candidate of candidates) {
    if (!candidate) {
      continue;
    }

    const hasPackage = fs.existsSync(path.join(candidate, "package.json"));
    const hasCargo = fs.existsSync(path.join(candidate, "src-tauri", "Cargo.toml"));

    if (hasPackage && hasCargo) {
      return candidate;
    }
  }

  return candidates.find(Boolean) || process.cwd();
}
