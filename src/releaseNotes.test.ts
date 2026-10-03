import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ReleaseHistoryError,
  changelogNotes,
  compareVersions,
  fetchReleaseHistory,
  notesForUpdateDialog,
  resetReleaseHistoryCache,
  versionFromTag,
} from "./releaseNotes";
import { REPO_API_URL } from "./repo";

/**
 * Corpo de uma release de hoje (v0.1.10 em diante): botão de download, a lista
 * simples em "What's Changed", a lista técnica recolhida e os contribuidores.
 */
const PLAIN_BODY = [
  "## Download",
  "",
  "### [⬇ Download MultiAlt-Setup.msi](https://github.com/luanmacea/roblox-account-manager/releases/download/v0.1.10-beta/MultiAlt-Setup.msi)",
  "",
  "Open the file and follow the installer. That's it.",
  "",
  "> [!NOTE]",
  "> This is a beta version: some things may still change.",
  "",
  "<details>",
  "<summary>Other files (you can ignore these)</summary>",
  "",
  "- `.sig` files: used by the automatic update.",
  "",
  "</details>",
  "",
  "## What's Changed",
  "- New menu on the left, with every screen named.",
  "- New language: Spanish.",
  "",
  "<details>",
  "<summary>Technical details</summary>",
  "",
  "* Sidebar navigation by @luanmacea in https://github.com/luanmacea/roblox-account-manager/pull/16",
  "",
  "",
  "**Full Changelog**: https://github.com/luanmacea/roblox-account-manager/compare/v0.1.9-beta...v0.1.10-beta",
  "",
  "</details>",
  "",
  "## Contributors",
  '<a href="https://github.com/luanmacea"><img src="https://github.com/luanmacea.png?size=64" width="32" height="32" alt="@luanmacea" /></a>',
  "",
  "[@luanmacea](https://github.com/luanmacea)",
].join("\n");

/** Corpo de uma release antiga (até a v0.1.9): a lista automática, com título de PR. */
const TECHNICAL_BODY = [
  "> [!WARNING]",
  "> This is a beta release. Missing features, bugs and crashes are possible. Run at your own risk.",
  "",
  "Channel: Beta",
  "Release commit: 15f422b",
  "App version: 0.1.9",
  "",
  "### Which file to download",
  "**Start here: `MultiAlt-Setup.msi`.** It installs for your user only.",
  "",
  "## What's Changed",
  "* README: blue download button and website link [skip release] by @luanmacea in https://github.com/luanmacea/roblox-account-manager/pull/11",
  "* Website deploys from main instead of develop [skip release] by @luanmacea in https://github.com/luanmacea/roblox-account-manager/pull/13",
  "* Rename to MultiAlt (Roblox Account Manager) by @luanmacea in https://github.com/luanmacea/roblox-account-manager/pull/15",
  "* Smoother updates by @someone-else in #9",
  "",
  "",
  "**Full Changelog**: https://github.com/luanmacea/roblox-account-manager/compare/v0.1.8-beta...v0.1.9-beta",
  "",
  "## Contributors",
  "[@luanmacea](https://github.com/luanmacea)",
].join("\n");

describe("notesForUpdateDialog (compartilhada com a janela de atualização)", () => {
  it("tira o download e os blocos recolhidos, e fica com a lista simples e os contribuidores", () => {
    const notes = notesForUpdateDialog(PLAIN_BODY);
    expect(notes.startsWith("## What's Changed")).toBe(true);
    expect(notes).toMatch(/New menu on the left/);
    expect(notes).not.toMatch(/Download MultiAlt|Technical details|<details>|Other files/);
    expect(notes).toMatch(/## Contributors/);
  });

  it("devolve o corpo intacto quando não há nada para tirar", () => {
    expect(notesForUpdateDialog(TECHNICAL_BODY)).toBe(TECHNICAL_BODY);
  });
});

describe("changelogNotes — o que a página de novidades mostra de cada versão", () => {
  it("de uma release de hoje, fica só com a lista simples", () => {
    expect(changelogNotes(PLAIN_BODY)).toBe(
      "- New menu on the left, with every screen named.\n- New language: Spanish."
    );
  });

  it("de uma release antiga, tira autor, número do PR e o link do comparativo", () => {
    const notes = changelogNotes(TECHNICAL_BODY);
    expect(notes).toBe("- Rename to MultiAlt (Roblox Account Manager)\n- Smoother updates");
    expect(notes).not.toMatch(/@luanmacea|pull\/|#9|Full Changelog|Contributors|Channel:/);
  });

  it("deixa de fora o que saiu com [skip release] (README, site), que não mudou o app", () => {
    const notes = changelogNotes(TECHNICAL_BODY);
    expect(notes).not.toMatch(/README|Website|skip release/);
  });

  it("entende a lista com marcador escapado e o comparativo sem negrito", () => {
    const body = [
      "## What's Changed",
      "",
      "\\* fix(update-dialog): render release notes by @niccdevs in #21",
      "* chore(ui): improve spacing",
      "",
      "Full Changelog: https://github.com/x/y/compare/v1...v2",
    ].join("\n");
    expect(changelogNotes(body)).toBe("- fix(update-dialog): render release notes\n- chore(ui): improve spacing");
  });

  it("não devolve nada quando a release não tem a seção What's Changed", () => {
    const body = [
      "Channel: Beta",
      "### Which file to download",
      "- **The `.msi`**: recommended.",
      "",
      "**Full Changelog**: https://github.com/x/y/compare/v0.1.1-beta...v0.1.2-beta",
      "",
      "## Contributors",
      "[@luanmacea](https://github.com/luanmacea)",
    ].join("\n");
    expect(changelogNotes(body)).toBe("");
  });

  it("não devolve nada quando a seção só tinha itens [skip release]", () => {
    const body = "## What's Changed\n* README tweak [skip release] by @a in #3\n\n**Full Changelog**: https://x/compare/a...b";
    expect(changelogNotes(body)).toBe("");
  });

  it("não mostra o aviso de notas que não puderam ser geradas", () => {
    expect(changelogNotes("## What's Changed\n- Release notes could not be generated.")).toBe("");
  });

  it("aceita fim de linha do Windows", () => {
    expect(changelogNotes("## What's Changed\r\n- One\r\n- Two\r\n")).toBe("- One\n- Two");
  });
});

describe("versões", () => {
  it("tira o v e o sufixo do canal da tag", () => {
    expect(versionFromTag("v0.1.10-beta")).toBe("0.1.10");
    expect(versionFromTag("0.2.0")).toBe("0.2.0");
    expect(versionFromTag("v1.0.0+build.5")).toBe("1.0.0");
  });

  it("compara número a número, não como texto", () => {
    expect(compareVersions("0.1.10", "0.1.9")).toBeGreaterThan(0);
    expect(compareVersions("0.1.9", "0.1.10")).toBeLessThan(0);
    expect(compareVersions("v0.1.10-beta", "0.1.10")).toBe(0);
    expect(compareVersions("1.0", "1.0.0")).toBe(0);
  });
});

function release(tag: string, publishedAt: string, body: string, extra: Record<string, unknown> = {}) {
  return {
    tag_name: tag,
    name: `MultiAlt ${tag}`,
    published_at: publishedAt,
    html_url: `https://github.com/luanmacea/roblox-account-manager/releases/tag/${tag}`,
    draft: false,
    prerelease: false,
    body,
    ...extra,
  };
}

function jsonResponse(data: unknown, init: { status?: number; headers?: Record<string, string> } = {}) {
  const status = init.status ?? 200;
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers(init.headers ?? {}),
    json: async () => data,
  } as unknown as Response;
}

describe("fetchReleaseHistory", () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    resetReleaseHistoryCache();
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("lê as releases do repositório, mais nova primeiro, sem rascunho e sem release vazia", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse([
        release("v0.1.9-beta", "2026-10-03T08:01:20Z", TECHNICAL_BODY),
        release("v0.1.10-beta", "2026-10-03T22:20:37Z", PLAIN_BODY),
        release("v0.1.11-beta", "2026-10-04T10:00:00Z", "## What's Changed\n- Draft", { draft: true }),
        release("v0.1.2-beta", "2026-09-28T18:01:45Z", "Channel: Beta\n\n## Contributors\n[@a](https://github.com/a)"),
      ])
    );

    const entries = await fetchReleaseHistory();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(String(fetchMock.mock.calls[0][0])).toBe(`${REPO_API_URL}/releases?per_page=30`);
    expect(entries.map((e) => e.version)).toEqual(["0.1.10", "0.1.9"]);
    expect(entries[0]).toMatchObject({
      tag: "v0.1.10-beta",
      publishedAt: "2026-10-03T22:20:37Z",
      notes: "- New menu on the left, with every screen named.\n- New language: Spanish.",
    });
  });

  it("guarda a lista na sessão: abrir a página de novo não pede outra vez", async () => {
    fetchMock.mockResolvedValue(jsonResponse([release("v0.1.10-beta", "2026-10-03T22:20:37Z", PLAIN_BODY)]));
    await fetchReleaseHistory();
    await fetchReleaseHistory();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("duas aberturas ao mesmo tempo dividem o mesmo pedido", async () => {
    fetchMock.mockResolvedValue(jsonResponse([]));
    await Promise.all([fetchReleaseHistory(), fetchReleaseHistory()]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("avisa quando o GitHub está limitando os pedidos", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ message: "API rate limit exceeded" }, {
      status: 403,
      headers: { "x-ratelimit-remaining": "0" },
    }));
    await expect(fetchReleaseHistory()).rejects.toMatchObject({ kind: "rate-limited" });
  });

  it("trata 429 como limite também", async () => {
    fetchMock.mockResolvedValue(jsonResponse({}, { status: 429 }));
    await expect(fetchReleaseHistory()).rejects.toBeInstanceOf(ReleaseHistoryError);
    resetReleaseHistoryCache();
    await expect(fetchReleaseHistory()).rejects.toMatchObject({ kind: "rate-limited" });
  });

  it("sem internet, o erro é de conexão — e a falha não fica guardada", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));
    await expect(fetchReleaseHistory()).rejects.toMatchObject({ kind: "unavailable" });

    fetchMock.mockResolvedValueOnce(jsonResponse([release("v0.1.10-beta", "2026-10-03T22:20:37Z", PLAIN_BODY)]));
    await expect(fetchReleaseHistory()).resolves.toHaveLength(1);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("resposta que não é lista conta como falha", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ message: "Not Found" }, { status: 404 }));
    await expect(fetchReleaseHistory()).rejects.toMatchObject({ kind: "unavailable" });
    resetReleaseHistoryCache();
    fetchMock.mockResolvedValue(jsonResponse({ weird: true }));
    await expect(fetchReleaseHistory()).rejects.toMatchObject({ kind: "unavailable" });
  });
});
