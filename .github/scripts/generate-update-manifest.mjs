import { execSync } from "node:child_process";
import { writeFileSync, mkdirSync, readFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { msiOnlyPlatforms } from "./update-manifest-platforms.mjs";

const tag = process.env.RELEASE_TAG;
const channel = process.env.RELEASE_CHANNEL || "beta";
const repo = process.env.GITHUB_REPOSITORY;
const manifestPath = process.env.RELEASE_MANIFEST_PATH || "";

if (!tag || !repo) {
  console.error("RELEASE_TAG and GITHUB_REPOSITORY are required");
  process.exit(1);
}

const version = tag.startsWith("v") ? tag.slice(1) : tag;

const manifest = manifestPath
  ? JSON.parse(readFileSync(manifestPath, "utf-8"))
  : JSON.parse(
      execSync(`gh release download "${tag}" --repo "${repo}" --pattern "latest.json" --output -`, {
        encoding: "utf-8",
      })
    );

manifest.version = version;
manifest.platforms = msiOnlyPlatforms(manifest.platforms);

const releaseJson = execSync(`gh release view "${tag}" --repo "${repo}" --json body,publishedAt`, {
  encoding: "utf-8",
});
const releaseData = JSON.parse(releaseJson);

manifest.notes = releaseData.body || manifest.notes || "";

if (!manifest.pub_date) {
  manifest.pub_date = releaseData.publishedAt || new Date().toISOString();
}

const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
if (!token) {
  console.error("GH_TOKEN or GITHUB_TOKEN is required for pushing");
  process.exit(1);
}

const tmpDir = mkdtempSync(join(tmpdir(), "ram-update-manifest-"));

const remote = `https://x-access-token:${token}@github.com/${repo}.git`;

try {
  // O branch `update-manifests` e um orfao: so os `latest.json`, sem historico
  // de codigo. Num repositorio que nunca publicou (ou recem-bifurcado) ele nao
  // existe, e o clone falharia no primeiro release — entao ele e criado aqui.
  let temBranch = true;
  try {
    execSync(`git ls-remote --exit-code --heads "${remote}" update-manifests`, { stdio: "ignore" });
  } catch {
    temBranch = false;
  }

  if (temBranch) {
    execSync(`git clone --depth 1 --branch update-manifests --single-branch "${remote}" "${tmpDir}"`, {
      stdio: "inherit",
    });
  } else {
    console.log("Branch update-manifests nao existe ainda; criando");
    execSync(`git -C "${tmpDir}" init -b update-manifests`, { stdio: "inherit" });
    execSync(`git -C "${tmpDir}" remote add origin "${remote}"`, { stdio: "inherit" });
    writeFileSync(
      join(tmpDir, "README.md"),
      "Manifestos do updater: um latest.json por canal. Gerado pelo release; nao editar a mao.\n"
    );
  }

  const channelDir = join(tmpDir, channel);
  mkdirSync(channelDir, { recursive: true });
  writeFileSync(join(channelDir, "latest.json"), JSON.stringify(manifest, null, 2) + "\n");

  execSync(`git -C "${tmpDir}" add -A`, { stdio: "inherit" });

  const status = execSync(`git -C "${tmpDir}" status --porcelain`, { encoding: "utf-8" }).trim();
  if (!status) {
    console.log("No changes to update manifest");
  } else {
    execSync(`git -C "${tmpDir}" commit -m "update ${channel}/latest.json to ${version}"`, {
      stdio: "inherit",
    });
    execSync(`git -C "${tmpDir}" push -u origin update-manifests`, { stdio: "inherit" });

    console.log(`Updated ${channel}/latest.json to ${version}`);
  }
} finally {
  rmSync(tmpDir, { recursive: true, force: true });
}
