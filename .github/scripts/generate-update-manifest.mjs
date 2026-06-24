import { execSync } from "node:child_process";
import { writeFileSync, mkdirSync, readFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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

try {
  execSync(`git clone --depth 1 --branch update-manifests --single-branch "https://x-access-token:${token}@github.com/${repo}.git" "${tmpDir}"`, {
    stdio: "inherit",
  });

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
    execSync(`git -C "${tmpDir}" push origin update-manifests`, { stdio: "inherit" });

    console.log(`Updated ${channel}/latest.json to ${version}`);
  }
} finally {
  rmSync(tmpDir, { recursive: true, force: true });
}
