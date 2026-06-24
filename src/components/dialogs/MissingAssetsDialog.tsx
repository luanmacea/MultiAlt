import { useState, useEffect, useCallback } from "react";
import { X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";

interface AssetInfo {
  id: number;
  name: string;
  isForSale: boolean;
  price: number | null;
  productId: number | null;
  creatorId: number;
  thumbnailUrl: string | null;
}

export function MissingAssetsDialog() {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const data = store.missingAssets;
  const [assets, setAssets] = useState<AssetInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [purchasing, setPurchasing] = useState<Set<number>>(new Set());
  const [closing, setClosing] = useState(false);

  const handleClose = useCallback(() => {
    setClosing(true);
    setTimeout(() => {
      setClosing(false);
      store.setMissingAssets(null);
    }, 100);
  }, [store]);

  useEffect(() => {
    if (!data) return;
    setLoading(true);
    setAssets([]);
    setPurchasing(new Set());

    (async () => {
      const infos: AssetInfo[] = [];

      try {
        const thumbs = await invoke<{ targetId: number; imageUrl: string | null }[]>(
          "get_asset_thumbnails",
          { assetIds: data.assetIds, size: "75x75", userId: data.userId }
        );
        const thumbMap = new Map(thumbs.map((t) => [t.targetId, t.imageUrl]));

        for (const assetId of data.assetIds) {
          try {
            const detail = await invoke<{
              Id: number;
              Name: string;
              IsForSale: boolean;
              PriceInRobux: number | null;
              ProductId: number | null;
              Creator: { Id: number };
            }>("get_asset_details", { assetId, userId: data.userId });
            infos.push({
              id: detail.Id,
              name: detail.Name,
              isForSale: detail.IsForSale,
              price: detail.PriceInRobux,
              productId: detail.ProductId,
              creatorId: detail.Creator.Id,
              thumbnailUrl: thumbMap.get(assetId) || null,
            });
          } catch {
            infos.push({
              id: assetId,
              name: t("Asset {{id}}", { id: assetId }),
              isForSale: false,
              price: null,
              productId: null,
              creatorId: 0,
              thumbnailUrl: thumbMap.get(assetId) || null,
            });
          }
        }
      } catch {}

      setAssets(infos);
      setLoading(false);
    })();

    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") handleClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [data]);

  if (!data) return null;

  async function handleBuy(asset: AssetInfo) {
    if (!asset.productId || asset.price === null) return;
    if (!(await confirm(t("Buy \"{{name}}\" for R$ {{price}}?", { name: asset.name, price: asset.price })))) return;

    setPurchasing((prev) => new Set([...prev, asset.id]));
    try {
      const result = await invoke<{ purchased: boolean; errorMsg: string | null }>(
        "purchase_product",
        {
          userId: data!.userId,
          productId: asset.productId,
          expectedPrice: asset.price,
          expectedSellerId: asset.creatorId,
        }
      );
      if (result.purchased) {
        store.addToast(t("Purchased {{name}}", { name: asset.name }));
      } else {
        store.addToast(result.errorMsg || t("Purchase failed"));
      }
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setPurchasing((prev) => {
      const next = new Set(prev);
      next.delete(asset.id);
      return next;
    });
  }

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      onClick={handleClose}
    >
      <div
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[440px] max-h-[400px] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <h2 className="text-sm font-semibold text-zinc-100">
            {t("Missing Assets for {{username}}", { username: data.username })}
          </h2>
          <button
            onClick={handleClose}
            className="text-zinc-600 hover:text-zinc-400 transition-colors"
          >
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 pb-4 space-y-1.5">
          {loading && (
            <p className="text-[11px] text-zinc-500 text-center py-6">{t("Loading asset details...")}</p>
          )}
          {!loading && assets.length === 0 && (
            <p className="text-[11px] text-zinc-500 text-center py-6">{t("No missing assets")}</p>
          )}
          {assets.map((asset) => (
            <div
              key={asset.id}
              className="flex items-center gap-3 p-2 bg-zinc-800/30 rounded-lg"
            >
              {asset.thumbnailUrl ? (
                <img
                  src={asset.thumbnailUrl}
                  alt=""
                  className="w-10 h-10 rounded bg-zinc-700 object-cover shrink-0"
                />
              ) : (
                <div className="w-10 h-10 rounded bg-zinc-700 shrink-0" />
              )}
              <div className="flex-1 min-w-0">
                <div className="text-xs text-zinc-200 truncate">{asset.name}</div>
                <div className="text-[10px] text-zinc-500">
                  {asset.price !== null ? `R$ ${asset.price}` : t("Not for sale")}
                </div>
              </div>
              {asset.isForSale && asset.productId && (
                <button
                  onClick={() => handleBuy(asset)}
                  disabled={purchasing.has(asset.id)}
                  className="px-2.5 py-1 bg-emerald-600/80 hover:bg-emerald-500 disabled:bg-zinc-700 text-white text-[10px] font-medium rounded-lg transition-colors shrink-0"
                >
                  {purchasing.has(asset.id) ? "..." : t("Buy")}
                </button>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
