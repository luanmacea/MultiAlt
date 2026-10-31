import { describe, expect, it } from "vitest";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  linuxAssetName,
  linuxManifest,
  linuxManifestChannel,
  linuxSignatureName,
  stageLinuxAssets,
} from "./linux-release-assets.mjs";
import { linuxOnlyPlatforms, msiOnlyPlatforms, platformsForFamily } from "./update-manifest-platforms.mjs";

/**
 * Instaladores Linux (branch `linux`, 31/10/2026): AppImage + .deb + .rpm por
 * edicao, canais de updater proprios (`<canal>-linux`, `<canal>-linux-nexus-ws`)
 * e um job desligado no release-v4.yml. Nada disto pode mexer no Windows: o
 * MSI, o `latest.json` do Windows e os canais dele ficam como estavam.
 */
const read = (...parts) => readFileSync(path.resolve(__dirname, "..", "..", ...parts), "utf8").replace(/\r\n/g, "\n");
const release = read(".github", "workflows", "release-v4.yml");
const buildWorkflow = read(".github", "workflows", "linux-build.yml");

/** O job `linux` inteiro (do cabecalho ate o fim do arquivo). */
const linuxJob = release.slice(release.indexOf("\n  linux:\n"));
const linuxSteps = () => {
  const out = [];
  let current = null;
  for (const line of linuxJob.split("\n")) {
    const m = /^ {6}- name: (.+)$/.exec(line);
    if (m) {
      current = { name: m[1].trim(), lines: [] };
      out.push(current);
    } else if (current) {
      current.lines.push(line);
    }
  }
  return out;
};

describe("nomes dos instaladores Linux", () => {
  it("a padrao e MultiAlt_<v>_Linux_*, a completa MultiAlt_<v>_Full-Linux_*", () => {
    expect(linuxAssetName("2.5.0", "standard", "appimage")).toBe("MultiAlt_2.5.0_Linux_amd64.AppImage");
    expect(linuxAssetName("2.5.0", "standard", "deb")).toBe("MultiAlt_2.5.0_Linux_amd64.deb");
    expect(linuxAssetName("2.5.0", "standard", "rpm")).toBe("MultiAlt_2.5.0_Linux_x86_64.rpm");
    expect(linuxAssetName("2.5.0", "full", "appimage")).toBe("MultiAlt_2.5.0_Full-Linux_amd64.AppImage");
    expect(linuxAssetName("2.5.0", "full", "rpm")).toBe("MultiAlt_2.5.0_Full-Linux_x86_64.rpm");
  });

  it("a completa tem _Full- no nome (o site descarta, so aparece nos anexos)", () => {
    for (const kind of ["appimage", "deb", "rpm"]) {
      expect(linuxAssetName("1.0.0", "full", kind)).toMatch(/_full-/i);
      expect(linuxAssetName("1.0.0", "standard", kind)).not.toMatch(/_full-/i);
    }
  });

  it("recusa edicao e formato desconhecidos", () => {
    expect(() => linuxAssetName("1.0.0", "lite", "deb")).toThrow(/edition/i);
    expect(() => linuxAssetName("1.0.0", "full", "msi")).toThrow(/format/i);
  });

  it("a assinatura sobe como zz-<arquivo>.sig", () => {
    expect(linuxSignatureName("a.AppImage")).toBe("zz-a.AppImage.sig");
  });
});

describe("canais do updater no Linux", () => {
  it("cada edicao tem o seu, sempre com o sufixo -linux", () => {
    expect(linuxManifestChannel("stable", "standard")).toBe("stable-linux");
    expect(linuxManifestChannel("beta", "standard")).toBe("beta-linux");
    expect(linuxManifestChannel("stable", "full")).toBe("stable-linux-nexus-ws");
    expect(linuxManifestChannel("beta", "full")).toBe("beta-linux-nexus-ws");
  });

  it("nenhum canal Linux e um canal do Windows", () => {
    const windows = ["stable", "beta", "stable-nexus-ws", "beta-nexus-ws"];
    for (const channel of ["stable", "beta"]) {
      for (const edition of ["standard", "full"]) {
        expect(windows).not.toContain(linuxManifestChannel(channel, edition));
      }
    }
  });

  it("o app escolhe o mesmo sufixo (updater.rs)", () => {
    const rust = read("src-tauri", "src", "commands", "updater.rs");
    expect(rust).toContain('if cfg!(target_os = "linux") { "-linux" } else { "" }');
    expect(rust).toContain("format!(\"{}-nexus-ws\", base)");
  });
});

describe("manifesto do updater do Linux", () => {
  const entry = (name) => ({ signature: `sig-${name}`, url: `https://github.com/o/r/releases/download/v1.0.0/${name}` });

  it("so aceita chaves linux-x86_64 do formato certo, com o AppImage obrigatorio", () => {
    const platforms = {
      "linux-x86_64-appimage": entry("a.AppImage"),
      "linux-x86_64-deb": entry("a.deb"),
      "linux-x86_64-rpm": entry("a.rpm"),
      "windows-x86_64-msi": entry("a.msi"),
    };
    expect(Object.keys(linuxOnlyPlatforms(platforms)).sort()).toEqual([
      "linux-x86_64-appimage",
      "linux-x86_64-deb",
      "linux-x86_64-rpm",
    ]);
    expect(() => linuxOnlyPlatforms({ "linux-x86_64-deb": entry("a.deb") })).toThrow(/appimage/i);
    expect(() => linuxOnlyPlatforms({ "linux-x86_64-appimage": entry("a.deb") })).toThrow(/appimage/i);
  });

  it("sem a familia, o manifesto continua sendo o do Windows (so o MSI)", () => {
    const msi = { "windows-x86_64-msi": entry("MultiAlt-Setup.msi") };
    expect(platformsForFamily(undefined, msi)).toEqual(msiOnlyPlatforms(msi));
    expect(platformsForFamily("", msi)).toEqual(msiOnlyPlatforms(msi));
    expect(() => platformsForFamily("linux", msi)).toThrow(/appimage/i);
    expect(() => platformsForFamily(undefined, { "linux-x86_64-appimage": entry("a.AppImage") })).toThrow(/msi/i);
  });

  it("linuxManifest aponta cada chave para o arquivo da release", () => {
    const manifest = linuxManifest({
      version: "2.5.0",
      repo: "o/r",
      tag: "v2.5.0",
      staged: [
        { kind: "appimage", name: "MultiAlt_2.5.0_Linux_amd64.AppImage", signature: "s1" },
        { kind: "deb", name: "MultiAlt_2.5.0_Linux_amd64.deb", signature: "s2" },
      ],
    });
    expect(manifest.version).toBe("2.5.0");
    expect(manifest.platforms["linux-x86_64-appimage"]).toEqual({
      signature: "s1",
      url: "https://github.com/o/r/releases/download/v2.5.0/MultiAlt_2.5.0_Linux_amd64.AppImage",
    });
    expect(Object.keys(manifest.platforms)).not.toContain("windows-x86_64-msi");
    expect(() => linuxManifest({ version: "1", repo: "o/r", tag: "v1", staged: [{ kind: "deb", name: "x" }] })).toThrow(/signature/i);
  });
});

describe("stageLinuxAssets", () => {
  const fixture = (withSig) => {
    const root = mkdtempSync(path.join(tmpdir(), "multialt-linux-"));
    for (const [dir, file] of [
      ["appimage", "MultiAlt_2.0.0_amd64.AppImage"],
      ["deb", "MultiAlt_2.0.0_amd64.deb"],
      ["rpm", "MultiAlt-2.0.0-1.x86_64.rpm"],
    ]) {
      mkdirSync(path.join(root, "bundle", dir), { recursive: true });
      writeFileSync(path.join(root, "bundle", dir, file), `content-${dir}`);
      if (withSig) writeFileSync(path.join(root, "bundle", dir, `${file}.sig`), `signature-${dir}\n`);
    }
    return root;
  };

  it("renomeia os tres pacotes e copia as assinaturas como zz-*.sig", () => {
    const root = fixture(true);
    try {
      const staged = stageLinuxAssets({
        bundleRoot: path.join(root, "bundle"),
        outDir: path.join(root, "out"),
        version: "2.5.0",
        edition: "full",
      });
      expect(readdirSync(path.join(root, "out")).sort()).toEqual([
        "MultiAlt_2.5.0_Full-Linux_amd64.AppImage",
        "MultiAlt_2.5.0_Full-Linux_amd64.deb",
        "MultiAlt_2.5.0_Full-Linux_x86_64.rpm",
        "zz-MultiAlt_2.5.0_Full-Linux_amd64.AppImage.sig",
        "zz-MultiAlt_2.5.0_Full-Linux_amd64.deb.sig",
        "zz-MultiAlt_2.5.0_Full-Linux_x86_64.rpm.sig",
      ]);
      expect(staged.find((s) => s.kind === "deb").signature).toBe("signature-deb");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("sem assinatura falha, a menos que seja build de teste", () => {
    const root = fixture(false);
    try {
      const args = { bundleRoot: path.join(root, "bundle"), outDir: path.join(root, "out"), version: "t", edition: "standard" };
      expect(() => stageLinuxAssets(args)).toThrow(/signature/i);
      const staged = stageLinuxAssets({ ...args, sign: false });
      expect(staged.map((s) => s.name)).toEqual([
        "MultiAlt_t_Linux_amd64.AppImage",
        "MultiAlt_t_Linux_amd64.deb",
        "MultiAlt_t_Linux_x86_64.rpm",
      ]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("falha se faltar um pacote ou se sobrar mais de um (sobra de build anterior)", () => {
    const root = fixture(true);
    try {
      writeFileSync(path.join(root, "bundle", "deb", "outro.deb"), "x");
      expect(() =>
        stageLinuxAssets({ bundleRoot: path.join(root, "bundle"), outDir: path.join(root, "out"), version: "1", edition: "standard" })
      ).toThrow(/exactly one \.deb/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("tauri.linux.conf.json", () => {
  const linux = JSON.parse(read("src-tauri", "tauri.linux.conf.json"));
  const root = JSON.parse(read("src-tauri", "tauri.conf.json"));

  it("gera AppImage, deb e rpm; o Windows continua so com o MSI", () => {
    expect(linux.bundle.targets).toEqual(["appimage", "deb", "rpm"]);
    expect(root.bundle.targets).toEqual(["msi"]);
  });

  it("o deb depende de WebKitGTK e GTK e recomenda flatpak e passt (pasta, para varias contas)", () => {
    expect(linux.bundle.linux.deb.depends).toEqual(expect.arrayContaining(["libwebkit2gtk-4.1-0", "libgtk-3-0"]));
    expect(linux.bundle.linux.deb.recommends).toEqual(expect.arrayContaining(["flatpak", "passt"]));
    expect(linux.bundle.linux.rpm.recommends).toEqual(expect.arrayContaining(["flatpak", "passt"]));
  });

  it("nao mexe no que o Windows usa", () => {
    expect(linux.bundle.windows).toBeUndefined();
    expect(linux.plugins).toBeUndefined();
    expect(linux.version).toBeUndefined();
  });
});

describe("job linux do release-v4.yml", () => {
  it("existe uma vez, depois do job release, e vem DESLIGADO", () => {
    expect(release.match(/^ {2}linux:$/gm)).toHaveLength(1);
    expect(linuxJob).toContain("needs: release");
    expect(release.match(/^ {6}PUBLISH_LINUX: /gm)).toHaveLength(1);
    expect(linuxJob).toContain('PUBLISH_LINUX: "false"');
  });

  it("todo passo (menos o aviso do interruptor) so roda com o interruptor ligado", () => {
    const steps = linuxSteps();
    expect(steps.length).toBeGreaterThan(8);
    for (const step of steps) {
      if (step.name === "Linux switch") continue;
      expect(step.lines.join("\n"), step.name).toContain("if: env.PUBLISH_LINUX == 'true'");
    }
  });

  it("o job release so ganhou `outputs` (tag, versao, canal) e continua publicando so o MSI", () => {
    const releaseJob = release.slice(release.indexOf("\n  release:\n"), release.indexOf("\n  # ── Linux"));
    expect(releaseJob).toContain("outputs:\n      tag: ${{ steps.meta.outputs.tag }}");
    expect(releaseJob).toContain('$targets = @("msi")');
    expect(releaseJob).not.toMatch(/AppImage|ubuntu|.deb/);
  });

  it("compila as duas edicoes como o Windows (standard sem lista, full com toda feature)", () => {
    const text = linuxJob;
    expect(text).toContain("bun tauri build -- --no-default-features --features standard");
    expect(text).toContain("bun tauri build -- --features full");
    expect(text).not.toMatch(/--features [\w-]+,/);
  });

  it("cada edicao publica no seu canal, separado do Windows", () => {
    expect(linuxJob).toContain("update-manifests/${{ needs.release.outputs.channel }}-linux/latest.json");
    expect(linuxJob).toContain("update-manifests/${{ needs.release.outputs.channel }}-linux-nexus-ws/latest.json");
    expect(linuxJob).toContain("RELEASE_CHANNEL: ${{ format('{0}-linux', needs.release.outputs.channel) }}");
    expect(linuxJob).toContain("RELEASE_CHANNEL: ${{ format('{0}-linux-nexus-ws', needs.release.outputs.channel) }}");
    expect(linuxJob.match(/RELEASE_PLATFORM_FAMILY: linux/g)).toHaveLength(2);
  });

  it("os instaladores sobem antes dos manifestos", () => {
    const names = linuxSteps().map((s) => s.name);
    const upload = names.findIndex((n) => /Upload Linux installers/.test(n));
    const manifests = names.map((n, i) => (/update manifest/.test(n) ? i : -1)).filter((i) => i >= 0);
    expect(upload).toBeGreaterThan(-1);
    expect(manifests).toHaveLength(2);
    for (const i of manifests) expect(i).toBeGreaterThan(upload);
  });

  it("nao recalcula a versao (a tag ja existe) e nao baixa nada da release", () => {
    expect(linuxJob).not.toContain("prepare-release.mjs");
    expect(linuxJob).toContain("prepare-linux-build.mjs");
    expect(linuxJob).not.toContain("gh release download");
  });
});

describe("linux-build.yml (build de teste)", () => {
  it("roda na branch linux e sob demanda, sem publicar nem usar segredo", () => {
    expect(buildWorkflow).toContain("branches:\n      - linux");
    expect(buildWorkflow).toContain("workflow_dispatch:");
    expect(buildWorkflow).not.toMatch(/secrets\./);
    expect(buildWorkflow).not.toMatch(/gh release|update-manifests/);
    expect(buildWorkflow).not.toMatch(/branches:[\s\S]*- main/);
  });

  it("gera as duas edicoes sem assinatura de updater e sobe como artefato", () => {
    expect(buildWorkflow).toContain('"createUpdaterArtifacts":false');
    expect(buildWorkflow).toContain("-- --no-default-features --features standard");
    expect(buildWorkflow).toContain("-- --features full");
    expect(buildWorkflow.match(/actions\/upload-artifact@v4/g)).toHaveLength(2);
    expect(buildWorkflow).toContain("multialt-linux-standard");
    expect(buildWorkflow).toContain("multialt-linux-full");
  });
});
