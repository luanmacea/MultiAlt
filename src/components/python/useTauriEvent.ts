import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

/**
 * Subscribes to a backend event for the lifetime of the component. The handler
 * can change on every render without resubscribing. If the component unmounts
 * before `listen` resolves, the late unlisten is called right away.
 */
export function useTauriEvent<T>(event: string, handler: (payload: T) => void): void {
  const handlerRef = useRef(handler);
  useEffect(() => {
    handlerRef.current = handler;
  });

  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | null = null;
    listen<T>(event, (e) => handlerRef.current(e.payload))
      .then((fn) => {
        if (cancelled) fn();
        else off = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      off?.();
    };
  }, [event]);
}
