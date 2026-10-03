/**
 * STUB TEMPORÁRIO — o AfkModeView de verdade (Auto Rejoin + cliques AFK em
 * abas) está sendo feito em outro ramo e substitui este arquivo inteiro na
 * integração. Aqui só existe a assinatura, para a página AFK Mode da barra
 * lateral (pages/AfkPage.tsx) já montar no lugar certo.
 */
export interface AfkModeViewProps {
  variant: "modal" | "page";
  initialTab?: "rejoin" | "clicks";
  targetUserIds?: number[];
  adoptRunning?: boolean;
  initialPlaceId?: string | null;
  onClose?: () => void;
}

export function AfkModeView({ variant }: AfkModeViewProps) {
  return <div data-testid="afk-mode-view" data-variant={variant} />;
}
