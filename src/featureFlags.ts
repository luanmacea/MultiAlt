function parseBooleanEnv(value: string | undefined, fallback: boolean): boolean {
  if (!value) return fallback;

  const normalized = value.trim().toLowerCase();
  if (["1", "true", "yes", "on"].includes(normalized)) return true;
  if (["0", "false", "no", "off"].includes(normalized)) return false;

  return fallback;
}

export const ENABLE_NEXUS = parseBooleanEnv(import.meta.env.VITE_ENABLE_NEXUS, true);
export const ENABLE_WEBSERVER = parseBooleanEnv(import.meta.env.VITE_ENABLE_WEBSERVER, true);

/**
 * Gerador de contas **pago** (BloxGen): a aba "Account Generator" do diálogo
 * Contas novas, a entrada "Account Generator" do menu Add (Toolbar e
 * AddAccountDialog) e a seção "Account Generator" das Settings. Desligado, o
 * diálogo Contas novas abre direto na criação grátis, sem seletor de abas — a
 * não ser que um gerador iniciado por fora (API de scripts) esteja rodando:
 * aí o painel pago aparece, para dar para pará-lo.
 *
 * Desligado desde 03/10/2026 porque o dono acha que o serviço do BloxGen parou
 * de funcionar, e ninguém deve gastar crédito tentando. O código continua
 * inteiro (comandos Rust, GeneratorTab, painel do diálogo); só as portas
 * somem. A criação **grátis** pelo formulário do Roblox ("Create Accounts")
 * não depende disto.
 *
 * Para religar: compile com `VITE_ENABLE_ACCOUNT_GENERATOR=true` ou troque o
 * padrão abaixo para `true`. Ver docs/features/account-creation.md.
 */
export const ENABLE_ACCOUNT_GENERATOR = parseBooleanEnv(
  import.meta.env.VITE_ENABLE_ACCOUNT_GENERATOR,
  false
);
