import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { FreeCatalogItem } from "../../../avatarBuilder";
import { thumbKey, type ThumbMap } from "./shared";

interface ThumbnailResponse {
  targetId: number;
  imageUrl: string | null;
  state?: string;
}

/** O Roblox às vezes responde "Pending" enquanto gera a imagem: tenta de novo algumas vezes. */
const THUMB_RETRY_MS = 2500;
const THUMB_MAX_RETRIES = 3;

/** Junta as entradas novas num mapa novo (o updater do `setState` tem de ser puro). */
export function mergeThumbs(prev: ThumbMap, updates: ThumbMap): ThumbMap {
  const next = new Map(prev);
  for (const [key, url] of updates) next.set(key, url);
  return next;
}

/**
 * Miniaturas 150x150 dos itens do catálogo, pedidas sob demanda pelo
 * `batch_thumbnails` (asset e bundle em pedidos separados) e guardadas pela
 * chave `kind:id`. Cada item é pedido uma vez; "Pending" é tentado de novo
 * depois de `retryMs`, até `maxRetries` vezes, e só então vira "sem imagem".
 */
export function useAvatarThumbs({
  retryMs = THUMB_RETRY_MS,
  maxRetries = THUMB_MAX_RETRIES,
}: { retryMs?: number; maxRetries?: number } = {}) {
  const [thumbs, setThumbs] = useState<ThumbMap>(new Map());
  const requestedRef = useRef<Set<string>>(new Set());
  const retriesRef = useRef<Map<string, number>>(new Map());
  const timersRef = useRef<Set<number>>(new Set());
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    const timers = timersRef.current;
    return () => {
      mountedRef.current = false;
      for (const timer of timers) window.clearTimeout(timer);
      timers.clear();
    };
  }, []);

  const ensureThumbs = useCallback(
    function ensure(items: FreeCatalogItem[]) {
      const missing = items.filter((item) => !requestedRef.current.has(thumbKey(item)));
      if (missing.length === 0) return;
      for (const item of missing) requestedRef.current.add(thumbKey(item));
      for (const kind of ["Asset", "Bundle"] as const) {
        const group = missing.filter((item) => item.kind === kind);
        if (group.length === 0) continue;
        const requests = group.map((item) => ({
          requestId: thumbKey(item),
          type: kind === "Asset" ? "Asset" : "BundleThumbnail",
          targetId: item.id,
          size: "150x150",
          format: "Png",
        }));
        invoke<ThumbnailResponse[]>("batch_thumbnails", { requests })
          .then((result) => {
            if (!mountedRef.current) return;
            const byId = new Map((Array.isArray(result) ? result : []).map((r) => [r.targetId, r]));
            // Tudo decidido aqui fora; o updater só junta.
            const updates: ThumbMap = new Map();
            const retry: FreeCatalogItem[] = [];
            for (const item of group) {
              const key = thumbKey(item);
              const hit = byId.get(item.id);
              if (hit?.imageUrl) {
                updates.set(key, hit.imageUrl);
                continue;
              }
              const tries = retriesRef.current.get(key) ?? 0;
              if (hit?.state === "Pending" && tries < maxRetries) {
                retriesRef.current.set(key, tries + 1);
                retry.push(item);
              } else {
                updates.set(key, null);
              }
            }
            if (updates.size > 0) setThumbs((prev) => mergeThumbs(prev, updates));
            if (retry.length > 0) {
              const timer = window.setTimeout(() => {
                timersRef.current.delete(timer);
                if (!mountedRef.current) return;
                for (const item of retry) requestedRef.current.delete(thumbKey(item));
                ensure(retry);
              }, retryMs);
              timersRef.current.add(timer);
            }
          })
          .catch(() => {
            if (!mountedRef.current) return;
            // Sem imagem agora; o pedido volta a valer na próxima vez que o item aparecer.
            for (const item of group) requestedRef.current.delete(thumbKey(item));
            setThumbs((prev) => mergeThumbs(prev, new Map(group.map((item) => [thumbKey(item), null]))));
          });
      }
    },
    [retryMs, maxRetries]
  );

  return { thumbs, ensureThumbs };
}
