import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { parsePlaceIdInput, type PlaceDetails } from "../components/server-list/types";

/**
 * Quem é o jogo por trás de um Place ID.
 *
 * Um número de 10 dígitos não diz nada a ninguém: a tela que trabalha com Place
 * ID tem que mostrar **qual jogo** é aquilo, sempre que for possível descobrir.
 * Este hook é o caminho único para isso — antes cada tela fazia por conta
 * (`GamesTab` chamava só o ícone num laço, a aba Servers legada só o nome, as
 * demais nada), sem cache nem dedupe: duas telas pedindo o mesmo place eram
 * dois pares de chamadas.
 *
 * Desenho:
 *
 * - **Cache de módulo por place**, lido de forma síncrona: a segunda tela que
 *   abre o mesmo jogo já nasce com o nome, sem piscar.
 * - **Espera a digitação parar** (`RESOLVE_DEBOUNCE_MS`): o campo muda a cada
 *   tecla e `6`, `60`, `606`… não são places que alguém queira consultar.
 * - **Falha não vira laço**: place que não resolveu fica marcado e só é tentado
 *   de novo depois de `RETRY_AFTER_MS`, então uma queda de rede não transforma
 *   a tela num gerador de requisições.
 * - **Resposta atrasada é descartada** se o usuário já trocou de place, senão o
 *   nome do jogo antigo apareceria carimbado no novo.
 * - Nome vazio/lista vazia é tratado como "não sei" (o harness devolve `[]` em
 *   cenário sem dublê, e a API devolve place sem nome para id inexistente).
 */
export interface GameIdentity {
  placeId: number;
  /** `null` enquanto não se sabe — a UI não deve inventar "Place 123". */
  name: string | null;
  iconUrl: string | null;
  loading: boolean;
}

interface CacheEntry {
  name: string | null;
  iconUrl: string | null;
  /** Instante da tentativa que não descobriu nada. */
  failedAt?: number;
}

const RESOLVE_DEBOUNCE_MS = 400;
const RETRY_AFTER_MS = 30_000;

const cache = new Map<number, CacheEntry>();
const inFlight = new Map<number, Promise<CacheEntry>>();

/** Esquece o que foi descoberto — par do `clear_image_cache` do backend. */
export function clearGameIdentityCache(): void {
  cache.clear();
  inFlight.clear();
}

/** O que já se sabe do place, ou `null` se nunca resolveu (ou já pode retentar). */
function cached(placeId: number): CacheEntry | null {
  const hit = cache.get(placeId);
  if (!hit) return null;
  if (hit.failedAt !== undefined && Date.now() - hit.failedAt >= RETRY_AFTER_MS) return null;
  return hit;
}

/**
 * Nome e ícone numa tacada. `allSettled` de propósito: ícone indisponível não
 * pode esconder o nome, e vice-versa.
 *
 * **Nunca rejeita.** Quem chama guarda o resultado no cache, e um erro precisa
 * virar "não sei" guardado — senão cada tela que abrisse o place tentaria de
 * novo na hora, e uma queda de rede viraria um gerador de requisições.
 */
async function resolveIdentity(placeId: number, userId: number | null): Promise<CacheEntry> {
  const [detailsRes, iconRes] = await Promise.allSettled([
    Promise.resolve().then(() =>
      invoke<PlaceDetails[]>("get_place_details", { placeIds: [placeId], userId })
    ),
    Promise.resolve().then(() =>
      invoke<string | null>("batched_get_game_icon", { placeId, userId })
    ),
  ]);

  let name: string | null = null;
  let iconUrl: string | null = null;
  if (detailsRes.status === "fulfilled" && Array.isArray(detailsRes.value)) {
    const found = detailsRes.value[0]?.name;
    if (typeof found === "string" && found.trim()) name = found.trim();
  }
  if (iconRes.status === "fulfilled" && typeof iconRes.value === "string" && iconRes.value) {
    iconUrl = iconRes.value;
  }

  const entry: CacheEntry = { name, iconUrl };
  if (!name && !iconUrl) entry.failedAt = Date.now();
  return entry;
}

/**
 * A mesma descoberta, fora do React (efeito, callback, backfill de lista).
 * Divide o cache e o dedupe com o hook: quem já perguntou por este place não
 * pergunta de novo, e quem falhou não é retentado antes de `RETRY_AFTER_MS`.
 */
export async function loadGameIdentity(
  placeId: number,
  userId: number | null = null
): Promise<GameIdentity> {
  const hit = cached(placeId);
  if (hit) {
    return { placeId, name: hit.name, iconUrl: hit.iconUrl, loading: false };
  }
  let pending = inFlight.get(placeId);
  if (!pending) {
    pending = resolveIdentity(placeId, userId).then((resolved) => {
      cache.set(placeId, resolved);
      inFlight.delete(placeId);
      return resolved;
    });
    inFlight.set(placeId, pending);
  }
  const resolved = await pending;
  return { placeId, name: resolved.name, iconUrl: resolved.iconUrl, loading: false };
}

/** Só places plausíveis viram consulta. Texto pode ser o link colado do jogo. */
function toPlaceId(input: string | number | null | undefined): number | null {
  if (typeof input === "number") {
    return Number.isSafeInteger(input) && input > 0 ? input : null;
  }
  if (!input) return null;
  return parsePlaceIdInput(String(input));
}

export function useGameIdentity(
  input: string | number | null | undefined,
  userId: number | null = null
): GameIdentity | null {
  const placeId = useMemo(() => toPlaceId(input), [input]);
  const [entry, setEntry] = useState<CacheEntry | null>(() => (placeId ? cached(placeId) : null));
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (placeId === null) {
      setEntry(null);
      setLoading(false);
      return;
    }

    const hit = cached(placeId);
    // Troca de place limpa o que estava na tela antes de perguntar o novo.
    setEntry(hit);
    if (hit) {
      setLoading(false);
      return;
    }

    setLoading(true);
    let alive = true;
    const timer = setTimeout(() => {
      void loadGameIdentity(placeId, userId).then((resolved) => {
        if (!alive) return;
        setEntry({ name: resolved.name, iconUrl: resolved.iconUrl });
        setLoading(false);
      });
    }, RESOLVE_DEBOUNCE_MS);

    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [placeId, userId]);

  if (placeId === null) return null;
  return {
    placeId,
    name: entry?.name ?? null,
    iconUrl: entry?.iconUrl ?? null,
    loading,
  };
}
