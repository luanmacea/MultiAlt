/**
 * Dublê de `@tauri-apps/api/core` para o harness de UI.
 *
 * Só existe no modo harness (`UI_HARNESS=1`); o alias está em `vite.config.ts`.
 */
import { harnessInvoke } from "./bus";
import "./scenarios";

export function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return harnessInvoke(cmd, args) as Promise<T>;
}

/** O app usa isto para imagens locais; no navegador a URL já serve. */
export function convertFileSrc(path: string): string {
  return path;
}

/** O app pergunta se está dentro do Tauri; no harness a resposta é sim. */
export function isTauri(): boolean {
  return true;
}
