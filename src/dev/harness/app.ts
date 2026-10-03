/**
 * Dublê de `@tauri-apps/api/app` para o harness de UI.
 *
 * O pacote real chama `plugin:app|version` direto no IPC do Tauri, que não
 * existe no navegador. Aqui a pergunta vai pelo `invoke` dos cenários com o
 * mesmo nome de comando, então um cenário responde a versão como responderia
 * qualquer outro comando.
 */
import { invoke } from "./core";

export function getVersion(): Promise<string> {
  return invoke<string>("plugin:app|version");
}
