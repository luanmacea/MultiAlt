import { useState, useCallback, useEffect, useRef } from "react";

export function useModalClose(open: boolean, onClose: () => void, duration = 100) {
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
