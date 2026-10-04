import { describe, expect, it } from "vitest";
import {
  RELEASE_KIND_BADGES,
  bumpForKind,
  classifyWhatsNew,
  decideReleaseKind,
  kindOverride,
  releaseKindHeader,
  resolveReleaseControls,
  whatsNewSection,
} from "./release-kind.mjs";
import { changelogNotes, notesForUpdateDialog, releaseKindOf } from "../../src/releaseNotes.ts";

const prBody = (...bullets) =>
  ["Some context for reviewers.", "", "## What's new", ...bullets.map((b) => `- ${b}`), "", "## Test plan", "- [x] bun run check"].join(
    "\n"
  );

describe("whatsNewSection", () => {
  it("pega só o que está sob ## What's new, até o próximo título", () => {
    expect(whatsNewSection(prBody("Fixed: a", "b"))).toBe("- Fixed: a\n- b");
  });

  it("sem a seção, volta vazio", () => {
    expect(whatsNewSection("## Summary\n- x")).toBe("");
    expect(whatsNewSection("")).toBe("");
    expect(whatsNewSection(undefined)).toBe("");
  });
});

describe("classifyWhatsNew", () => {
  it("todos com Fixed: é correção", () => {
    expect(classifyWhatsNew("- Fixed: one\n- fixed: two\n* FIXED:three")).toBe("fix");
  });

  it("nenhum com Fixed: é novidade", () => {
    expect(classifyWhatsNew("- New menu on the left\n- Spanish")).toBe("feature");
  });

  it("os dois é atualização geral", () => {
    expect(classifyWhatsNew("- Fixed: crash on start\n- New menu")).toBe("mixed");
  });

  it("Fixed no meio da frase não conta, Fixed em negrito conta", () => {
    expect(classifyWhatsNew("- The window is now fixed: no more jumping")).toBe("feature");
    expect(classifyWhatsNew("- **Fixed:** crash on start")).toBe("fix");
  });

  it("sem itens é atualização geral", () => {
    expect(classifyWhatsNew("")).toBe("mixed");
    expect(classifyWhatsNew("Just a paragraph, no list.")).toBe("mixed");
  });
});

describe("kindOverride", () => {
  it("lê a marca da mensagem do commit", () => {
    expect(kindOverride({ message: "Merge develop [type:fix]" })).toBe("fix");
    expect(kindOverride({ message: "[TYPE:Feature] big one" })).toBe("feature");
    expect(kindOverride({ message: "x [type:mixed]" })).toBe("mixed");
  });

  it("lê o rótulo do PR", () => {
    expect(kindOverride({ message: "", labels: ["bump:minor", "type:fix"] })).toBe("fix");
  });

  it("a mensagem vence o rótulo", () => {
    expect(kindOverride({ message: "[type:feature]", labels: ["type:fix"] })).toBe("feature");
  });

  it("sem marca nenhuma, null", () => {
    expect(kindOverride({ message: "Merge pull request #20", labels: ["channel:beta"] })).toBeNull();
    expect(kindOverride({})).toBeNull();
  });
});

describe("decideReleaseKind", () => {
  it("a marca vence a classificação dos itens", () => {
    expect(decideReleaseKind({ prBody: prBody("Fixed: a"), message: "[type:feature]" })).toBe("feature");
  });

  it("sem marca, classifica o What's new do PR", () => {
    expect(decideReleaseKind({ prBody: prBody("Fixed: a", "Fixed: b") })).toBe("fix");
    expect(decideReleaseKind({ prBody: prBody("New thing") })).toBe("feature");
    expect(decideReleaseKind({ prBody: prBody("Fixed: a", "New thing") })).toBe("mixed");
  });

  it("sem PR ou sem a seção, atualização geral", () => {
    expect(decideReleaseKind({ prBody: "" })).toBe("mixed");
    expect(decideReleaseKind({ prBody: "## Summary\n- Fixed: a" })).toBe("mixed");
  });
});

describe("bumpForKind", () => {
  it("correção soma patch; novidade e geral somam minor", () => {
    expect(bumpForKind("fix")).toBe("patch");
    expect(bumpForKind("feature")).toBe("minor");
    expect(bumpForKind("mixed")).toBe("minor");
  });
});

describe("resolveReleaseControls", () => {
  it("o tipo decide o bump", () => {
    expect(resolveReleaseControls({ prBody: prBody("Fixed: a") })).toEqual({ kind: "fix", bump: "patch", channel: "beta" });
    expect(resolveReleaseControls({ prBody: prBody("New") })).toEqual({ kind: "feature", bump: "minor", channel: "beta" });
    expect(resolveReleaseControls({})).toEqual({ kind: "mixed", bump: "minor", channel: "beta" });
  });

  it("[bump:minor] e o rótulo bump:minor continuam forçando minor", () => {
    expect(resolveReleaseControls({ prBody: prBody("Fixed: a"), message: "x [bump:minor]" }).bump).toBe("minor");
    expect(resolveReleaseControls({ prBody: prBody("Fixed: a"), labels: ["bump:minor"] }).bump).toBe("minor");
  });

  it("[bump:patch] força patch", () => {
    expect(resolveReleaseControls({ prBody: prBody("New"), message: "[bump:patch]" }).bump).toBe("patch");
    expect(resolveReleaseControls({ prBody: prBody("New"), labels: ["bump:patch"] }).bump).toBe("patch");
  });

  it("o bump escolhido à mão no workflow_dispatch vence; 'auto' deixa o tipo decidir", () => {
    expect(resolveReleaseControls({ prBody: prBody("New"), dispatchBump: "patch" }).bump).toBe("patch");
    expect(resolveReleaseControls({ prBody: prBody("Fixed: a"), dispatchBump: "minor" }).bump).toBe("minor");
    expect(resolveReleaseControls({ prBody: prBody("Fixed: a"), dispatchBump: "auto" }).bump).toBe("patch");
  });

  it("canal: marca na mensagem e rótulo, como antes", () => {
    expect(resolveReleaseControls({ message: "[channel:stable]" }).channel).toBe("stable");
    expect(resolveReleaseControls({ labels: ["channel:stable"] }).channel).toBe("stable");
    expect(resolveReleaseControls({ message: "[channel:stable]", labels: ["channel:beta"] }).channel).toBe("beta");
  });

  it("a marca de tipo também move o bump", () => {
    expect(resolveReleaseControls({ prBody: prBody("New"), message: "[type:fix]" })).toMatchObject({ kind: "fix", bump: "patch" });
  });
});

describe("releaseKindHeader", () => {
  it("abre com a marca para máquina e mostra o selo", () => {
    expect(releaseKindHeader("fix")).toBe("<!-- release-kind: fix -->\n**🩹 Hotfix**");
    expect(releaseKindHeader("feature")).toBe("<!-- release-kind: feature -->\n**✨ New features**");
    expect(releaseKindHeader("mixed")).toBe("<!-- release-kind: mixed -->\n**📦 General update**");
  });

  it("recusa tipo desconhecido", () => {
    expect(() => releaseKindHeader("major")).toThrow();
  });
});

/**
 * O app lê o tipo pela marca e não pode mostrar nem a marca nem o selo como
 * texto: os dois lados (workflow e src/releaseNotes.ts) têm que bater.
 */
describe("o app entende o cabeçalho que o workflow escreve", () => {
  const finalBody = (kind) =>
    [
      releaseKindHeader(kind),
      "",
      "## Download",
      "",
      "### [⬇ Download MultiAlt-Setup.msi](https://example.com/MultiAlt-Setup.msi)",
      "",
      "## What's Changed",
      "- Fixed: something",
      "",
      "## Contributors",
      "[@luanmacea](https://github.com/luanmacea)",
    ].join("\n");

  for (const kind of ["fix", "feature", "mixed"]) {
    it(`${kind}: lê o tipo e esconde marca e selo`, () => {
      const body = finalBody(kind);
      expect(releaseKindOf(body)).toBe(kind);
      const dialog = notesForUpdateDialog(body);
      expect(dialog).not.toMatch(/release-kind|Hotfix|New features|General update/);
      expect(dialog.startsWith("## What's Changed")).toBe(true);
      expect(changelogNotes(body)).toBe("- Fixed: something");
    });
  }

  it("o corpo inicial (prepare-release, antes do Finalize) também é lido", () => {
    const body = [releaseKindHeader("fix"), "", "> [!WARNING]", "> beta", "", "Channel: Beta"].join("\n");
    expect(releaseKindOf(body)).toBe("fix");
    expect(notesForUpdateDialog(body)).not.toMatch(/release-kind|Hotfix/);
  });

  it("os selos são os mesmos dos dois lados", () => {
    expect(Object.keys(RELEASE_KIND_BADGES).sort()).toEqual(["feature", "fix", "mixed"]);
  });
});
