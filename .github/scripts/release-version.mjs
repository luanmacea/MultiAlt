// Número da próxima release, separado do prepare-release.mjs para ser testado
// (release-version.test.mjs, suíte `release`).
//
// A série é o major do package.json: tags de outro major não contam (as v4.x,
// herdadas do número do projeto original, ficam fora da série 0.x).
// - Versão do package.json acima de todas as tags da série: sai como está. É
//   assim que se abre uma série (0.1.0) ou se escolhe um número (1.0.0).
// - Senão, soma patch/minor a partir da tag mais alta. O workflow não commita
//   o número de volta: o package.json fica parado e são as tags que sobem.
export function nextReleaseVersion({ current, tags, bump }) {
  const base = parseVersion(current);
  if (!base) {
    throw new Error(`Invalid current version in package.json: ${current}`);
  }

  const series = tags
    .map((tag) => parseTag(tag))
    .filter(Boolean)
    .filter((version) => version.major === base.major);

  const highest = series.length > 0 ? pickLatest(series) : null;
  const previousTag = highest?.tag || "";

  // Só o núcleo conta: a tag v0.1.0-beta já ocupa o número 0.1.0.
  if (!highest || compareCore(base, highest) > 0) {
    return { version: coreOf(base), previousTag };
  }

  const next =
    bump === "minor"
      ? { major: highest.major, minor: highest.minor + 1, patch: 0 }
      : { major: highest.major, minor: highest.minor, patch: highest.patch + 1 };
  return { version: coreOf(next), previousTag };
}

// Troca a versao do [package] no Cargo.toml. Mesma versao nao e erro: a
// primeira release de uma serie sai com o numero que ja esta no arquivo.
export function setCargoPackageVersion(cargoToml, version) {
  const line = /^version\s*=\s*".*"$/m;
  if (!line.test(cargoToml)) {
    throw new Error("Could not find the package version in Cargo.toml");
  }
  return cargoToml.replace(line, `version = "${version}"`);
}

function coreOf(version) {
  return `${version.major}.${version.minor}.${version.patch}`;
}

function compareCore(a, b) {
  if (a.major !== b.major) return a.major - b.major;
  if (a.minor !== b.minor) return a.minor - b.minor;
  return a.patch - b.patch;
}

function parseTag(tag) {
  if (!tag.startsWith("v")) {
    return null;
  }

  const parsed = parseVersion(tag.slice(1));
  if (!parsed) {
    return null;
  }

  return { ...parsed, tag };
}

function parseVersion(input) {
  const raw = String(input).trim();
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-beta(?:\.(\d+))?)?$/.exec(raw);
  if (!match) {
    return null;
  }

  const hasBeta = /-beta(?:\.\d+)?$/.test(raw);

  return {
    raw,
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    beta: hasBeta ? (match[4] ? Number(match[4]) : 0) : null
  };
}

function pickLatest(versions) {
  return versions.reduce((best, candidate) => (compareVersions(candidate, best) > 0 ? candidate : best));
}

function compareVersions(a, b) {
  if (a.major !== b.major) return a.major - b.major;
  if (a.minor !== b.minor) return a.minor - b.minor;
  if (a.patch !== b.patch) return a.patch - b.patch;

  const aStable = a.beta === null;
  const bStable = b.beta === null;

  if (aStable && !bStable) return 1;
  if (!aStable && bStable) return -1;
  if (aStable && bStable) return 0;

  return a.beta - b.beta;
}
