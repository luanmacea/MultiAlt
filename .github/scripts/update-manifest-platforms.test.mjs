import { describe, expect, it } from "vitest";
import { STABLE_MSI_NAME, msiOnlyPlatforms } from "./update-manifest-platforms.mjs";

const MSI = { signature: "sig-msi", url: "https://example/app.msi" };
const NSIS = { signature: "sig-nsis", url: "https://example/app-setup.exe" };

describe("msiOnlyPlatforms", () => {
  it("keeps only the msi key", () => {
    expect(
      msiOnlyPlatforms({
        "windows-x86_64": MSI,
        "windows-x86_64-msi": MSI,
        "windows-x86_64-nsis": NSIS,
      })
    ).toEqual({ "windows-x86_64-msi": MSI });
  });

  it("drops the generic key even when it points at the msi", () => {
    // A chave generica e a que um app sem formato conhecido (portatil, ou um
    // NSIS antigo sem a chave dele) usaria: mandar o MSI para ele instalaria
    // uma segunda copia ao lado da atual.
    expect(Object.keys(msiOnlyPlatforms({ "windows-x86_64": MSI, "windows-x86_64-msi": MSI }))).toEqual([
      "windows-x86_64-msi",
    ]);
  });

  it("takes the generic key as the msi when only it exists and it is an .msi", () => {
    expect(msiOnlyPlatforms({ "windows-x86_64": MSI })).toEqual({ "windows-x86_64-msi": MSI });
  });

  it("refuses a manifest without the msi", () => {
    expect(() => msiOnlyPlatforms({ "windows-x86_64": NSIS, "windows-x86_64-nsis": NSIS })).toThrow(/msi/i);
  });

  it("refuses an msi entry pointing at something that is not an .msi", () => {
    expect(() => msiOnlyPlatforms({ "windows-x86_64-msi": NSIS })).toThrow(/\.msi/);
  });
});

describe("STABLE_MSI_NAME", () => {
  it("is the fixed name the README download button links to", () => {
    expect(STABLE_MSI_NAME).toBe("MultiAlt-Setup.msi");
  });
});

/**
 * Um setup só na release (pedido do dono, 03/10/2026): o `MultiAlt_<v>_x64_en-US.msi`
 * que o tauri-action sobe é o mesmo arquivo do `MultiAlt-Setup.msi` (mesmo
 * sha256), então ele sai da release e o updater passa a baixar o de nome fixo.
 * A assinatura é a do conteúdo, então continua valendo.
 */
describe("msiOnlyPlatforms aponta o updater para o setup de nome fixo", () => {
  const versioned = {
    signature: "sig-msi",
    url: "https://github.com/luanmacea/MultiAlt/releases/download/v1.0.0/MultiAlt_1.0.0_x64_en-US.msi",
  };

  it("troca o nome do arquivo e mantém a tag e a assinatura", () => {
    expect(msiOnlyPlatforms({ "windows-x86_64-msi": versioned })).toEqual({
      "windows-x86_64-msi": {
        signature: "sig-msi",
        url: "https://github.com/luanmacea/MultiAlt/releases/download/v1.0.0/MultiAlt-Setup.msi",
      },
    });
  });

  it("deixa como está o MSI da versão completa (outro arquivo)", () => {
    const full = {
      signature: "sig-full",
      url: "https://github.com/luanmacea/MultiAlt/releases/download/v1.0.0/MultiAlt_1.0.0_Full-Setup.msi",
    };
    expect(msiOnlyPlatforms({ "windows-x86_64-msi": full })).toEqual({ "windows-x86_64-msi": full });
  });
});
