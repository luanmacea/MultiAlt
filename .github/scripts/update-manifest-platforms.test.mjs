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
