import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { VaultKeyBanner } from "./VaultKeyBanner";
import { renderWithStore } from "../../test-utils/renderWithStore";

afterEach(cleanup);

/**
 * A faixa é a **única** rede contra o lockout de quem usa a chave do aparelho:
 * sem senha, o `AccountData.key` é o que abre as contas, e se ele fica ruim o app
 * segue funcionando até o boot seguinte — quando não há senha para digitar.
 *
 * A primeira tentativa usava a linha de status, que é substituível (qualquer
 * "Launching…" apagava o aviso) e nunca era limpa. Daí ser faixa, e daí estes
 * testes travarem as duas pontas: aparece quando tem, **desaparece quando não**.
 */
describe("VaultKeyBanner", () => {
  it("não desenha nada quando não há problema", () => {
    renderWithStore(<VaultKeyBanner />, { vaultKeyWarning: null });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("avisa alto quando o arquivo de chave não pôde ser gravado", () => {
    renderWithStore(<VaultKeyBanner />, {
      vaultKeyWarning: {
        code: "writeFailed",
        path: "C:\\dados\\AccountData.key",
        detail: "acesso negado pelo sistema",
      },
    });

    const banner = screen.getByRole("alert");
    // O caminho tem que estar na tela: é o que o dono precisa para agir.
    expect(banner).toHaveTextContent("C:\\dados\\AccountData.key");
    // E a ação útil, que é tirar backup **agora**, enquanto o processo vive.
    expect(banner).toHaveTextContent(/make a backup now/i);
    expect(banner).toHaveTextContent(/may not open after you close it/i);
    // O detalhe técnico do SO aparece, para poder ser reportado.
    expect(banner).toHaveTextContent("acesso negado pelo sistema");
  });

  // Tom brando de propósito: quase sempre é antivírus segurando o arquivo por um
  // instante. Alarme falso treina o dono a ignorar alarme.
  it("é discreta quando a falha é transitória", () => {
    renderWithStore(<VaultKeyBanner />, {
      vaultKeyWarning: { code: "writeFailedTransient", path: "C:\\dados\\AccountData.key" },
    });

    const banner = screen.getByRole("alert");
    expect(banner).toHaveTextContent(/try again on the next change/i);
    expect(banner).not.toHaveTextContent(/make a backup now/i);
  });

  // O aviso mais consequente dos cinco: o dono acha que está criptografado e não
  // está, com o cookie de todas as contas legível no disco.
  it("diz que o arquivo ficou em texto puro quando a migração falha", () => {
    renderWithStore(<VaultKeyBanner />, {
      vaultKeyWarning: {
        code: "migrationFailed",
        path: "C:\dados\AccountData.json",
        detail: "acesso negado",
      },
    });

    const banner = screen.getByRole("alert");
    expect(banner).toHaveTextContent(/still plain text/i);
    expect(banner).toHaveTextContent(/cookie of every account is readable/i);
    // E tem que dizer o que fazer, sem sugerir que algo foi perdido.
    expect(banner).toHaveTextContent(/Nothing was lost/i);
    expect(banner).toHaveTextContent(/Change Encryption Method/i);
  });

  it("avisa sem alarme quando o disco não confirmou a gravação", () => {
    renderWithStore(<VaultKeyBanner />, {
      vaultKeyWarning: { code: "syncUnconfirmed", path: "C:\dados\AccountData.json" },
    });

    const banner = screen.getByRole("alert");
    expect(banner).toHaveTextContent(/did not confirm the write/i);
    expect(banner).not.toHaveTextContent(/make a backup now/i);
  });

  it("explica a degradação quando a chave ficou sem a proteção do Windows", () => {
    renderWithStore(<VaultKeyBanner />, {
      vaultKeyWarning: { code: "weakWrapper", path: "C:\\dados\\AccountData.key" },
    });

    const banner = screen.getByRole("alert");
    expect(banner).toHaveTextContent(/without Windows protection/i);
    expect(banner).toHaveTextContent(/still open/i);
  });
});
