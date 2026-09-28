import { invoke } from "@tauri-apps/api/core";
import { tr } from "../i18n/text";
import { parseImportLine } from "./cookies";
import type { ToastTone } from "./toastTone";

/** O pedaço da store que o Quick Add usa. */
export interface QuickAddStore {
  addAccountByCookie: (cookie: string, password?: string) => Promise<void>;
  loadAccounts: () => Promise<void>;
  addToast: (message: string, tone?: ToastTone) => void;
}

/**
 * O Quick Add — o mesmo no menu `Add` da toolbar e no diálogo Add Account, as
 * duas portas de quem quer colar uma conta. Morava copiado nas duas telas.
 *
 * A linha passa pelo leitor do import (`parseImportLine`), porque o import
 * anuncia o formato `username:password:cookie`. Antes, as duas telas decidiam
 * com `includes(COOKIE_MARKER)` e mandavam a linha **inteira** como cookie: o
 * que ia para o Roblox era `.ROBLOSECURITY=usuario:senha:_|WARNING…`, a senha
 * viajava no cabeçalho de cookie e o recurso falhava com "Invalid cookie". E
 * `usuario:senha` sem cookie ia para o `lookup_user` como se fosse um nome.
 *
 * - cookie (sozinho ou com `usuario:senha` na frente): só o cookie vai como
 *   cookie; a senha, se veio, é guardada separada — como o import faz;
 * - `usuario:senha` sem cookie: o Quick Add não entra com senha, e a frase diz
 *   qual entrada faz isso;
 * - o resto é nome de usuário: a conta entra sem sessão, e o aviso diz isso.
 */
export async function quickAddAccount(input: string, store: QuickAddStore): Promise<void> {
  const parsed = parseImportLine(input);
  if (parsed.kind === "empty") return;

  try {
    if (parsed.kind === "cookie") {
      if (parsed.password) {
        await store.addAccountByCookie(parsed.cookie, parsed.password);
      } else {
        await store.addAccountByCookie(parsed.cookie);
      }
      return;
    }

    if (parsed.kind === "userpass") {
      // Nome de usuário do Roblox não tem `:` — isto é uma credencial, e a
      // senha não pode sair numa busca de usuário.
      store.addToast(
        tr(
          "That line has no cookie. Quick Add takes a cookie or a username; to sign in with a username and password, use User:Pass Login."
        ),
        "warn"
      );
      return;
    }

    const user = await invoke<{ id: number; name: string }>("lookup_user", {
      username: input.trim(),
    });
    await invoke("add_account", {
      securityToken: "",
      username: user.name,
      userId: user.id,
    });
    await store.loadAccounts();
    // Busca por nome de usuário não entrega cookie nenhum: a conta entra só
    // como registro, sem sessão, e não lança. Dizer só "Added" fazia parecer
    // que tinha dado certo — o aviso tem que nomear o que falta.
    store.addToast(
      tr("Added {{name}} with no session — paste its cookie or use Browser Login to sign in", {
        name: user.name,
      })
    );
  } catch (e) {
    store.addToast(tr("Add failed: {{error}}", { error: String(e) }));
  }
}
