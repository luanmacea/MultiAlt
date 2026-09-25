import { useState, useCallback, useEffect, useRef } from "react";
import { useEscapeStack } from "./useEscapeStack";

/**
 * Estado de abrir/fechar de um diálogo — e, desde a pilha de Escape, também
 * **quem fecha com Escape**. Cada diálogo tinha (ou não tinha) o seu próprio
 * `window.addEventListener("keydown")`: seis não fechavam com Escape, e os que
 * fechavam disputavam o mesmo evento com a tela de trás.
 *
 * `onEscape` existe para o diálogo que precisa fazer algo antes de fechar — o
 * editor de temas reverte a pré-visualização.
 */
export function useModalClose(
  open: boolean,
  onClose: () => void,
  duration = 100,
  onEscape?: () => void
) {
  const [visible, setVisible] = useState(false);
  const [closing, setClosing] = useState(false);
  const onCloseRef = useRef(onClose);
  const closeTimeoutRef = useRef<number | null>(null);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    if (open) {
      if (closeTimeoutRef.current !== null) {
        window.clearTimeout(closeTimeoutRef.current);
        closeTimeoutRef.current = null;
      }
      setVisible(true);
      setClosing(false);
      return;
    }

    if (!visible) return;
    if (closeTimeoutRef.current !== null) {
      window.clearTimeout(closeTimeoutRef.current);
      closeTimeoutRef.current = null;
    }

    setClosing(true);
    closeTimeoutRef.current = window.setTimeout(() => {
      setClosing(false);
      setVisible(false);
      closeTimeoutRef.current = null;
    }, duration);
  }, [duration, open, visible]);

  const handleClose = useCallback(() => {
    if (open) {
      onCloseRef.current();
    }
  }, [open]);

  const onEscapeRef = useRef(onEscape);
  useEffect(() => {
    onEscapeRef.current = onEscape;
  }, [onEscape]);

  const escapeAction = useCallback(() => {
    if (onEscapeRef.current) onEscapeRef.current();
    else handleClose();
  }, [handleClose]);

  useEscapeStack(open, escapeAction);

  useEffect(() => {
    return () => {
      if (closeTimeoutRef.current !== null) {
        window.clearTimeout(closeTimeoutRef.current);
        closeTimeoutRef.current = null;
      }
    };
  }, []);

  return { visible, closing, handleClose };
}
