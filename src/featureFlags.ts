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
 * Gerador de contas **pago** (BloxGen): o painel do diálogo Account Generator,
 * a entrada "Account Generator" do menu Add (Toolbar e AddAccountDialog) e a
 * seção "Account Generator" das Settings. Desligado, o diálogo só aparece se um
 * gerador iniciado por fora (API de scripts) estiver rodando: aí o painel pago
 * aparece, para dar para pará-lo.
 *
 * Desligado desde 03/10/2026 porque o dono acha que o serviço do BloxGen parou
 * de funcionar, e ninguém deve gastar crédito tentando. O código continua
 * inteiro (comandos Rust, GeneratorTab, painel do diálogo); só as portas
 * somem.
 *
 * Para religar: compile com `VITE_ENABLE_ACCOUNT_GENERATOR=true` ou troque o
 * padrão abaixo para `true`.
 */
export const ENABLE_ACCOUNT_GENERATOR = parseBooleanEnv(
  import.meta.env.VITE_ENABLE_ACCOUNT_GENERATOR,
  false
);

/**
 * Botão "Help" no rodapé da barra lateral (refaz o tutorial). Escondido desde
 * 08/10/2026 a pedido do dono — pode voltar como FAQ. O código continua
 * inteiro; o tutorial segue em Settings › App › "Open Walkthrough".
 *
 * Para religar: compile com `VITE_ENABLE_HELP_BUTTON=true` ou troque o padrão
 * abaixo para `true`. Ver docs/features/ui-layout.md.
 */
export const ENABLE_HELP_BUTTON = parseBooleanEnv(import.meta.env.VITE_ENABLE_HELP_BUTTON, false);

/**
 * Página "Groups" da barra lateral. Ficou escondida de 08/10 a 13/10/2026: a
 * entrada pela API esbarrava sempre num quebra-cabeça do Roblox, e no
 * navegador da conta o botão de entrar ficava atrás do aviso de termos novos.
 * Desde 13/10/2026 a entrada é pelo navegador da própria conta (medido com
 * contas de teste: entra sem pedir nada), e a página voltou.
 *
 * Para esconder de novo: compile com `VITE_ENABLE_GROUPS=false` ou troque o
 * padrão abaixo para `false`. Ver docs/features/groups.md.
 */
export const ENABLE_GROUPS = parseBooleanEnv(import.meta.env.VITE_ENABLE_GROUPS, true);
