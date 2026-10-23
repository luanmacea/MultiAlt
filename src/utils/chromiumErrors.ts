/**
 * O navegador do criador de contas/login abriu mas não respondeu no prazo
 * (`BROWSER_DID_NOT_OPEN` em src-tauri/src/chromium/cdp.rs — o teste lê a
 * frase do fonte do Rust). Vem em inglês do backend; aqui vira a língua da
 * tela, mesmo dentro de um texto maior.
 */
const BROWSER_DID_NOT_OPEN =
  "The browser did not open in time. Check your antivirus and firewall, then try Reinstall in Settings > Accounts.";

export function chromiumErrorText(error: unknown, t: (text: string) => string): string {
  const text = String(error ?? "");
  return text.includes(BROWSER_DID_NOT_OPEN) ? text.replace(BROWSER_DID_NOT_OPEN, t(BROWSER_DID_NOT_OPEN)) : text;
}
