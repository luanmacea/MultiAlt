import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { X, Play, Pencil, Plus, Clock, Trash2, AlertTriangle } from "lucide-react";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useConfirm } from "../../hooks/usePrompt";
import { useJoinOnlineWarning } from "../../hooks/useJoinOnlineWarning";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import { useTr } from "../../i18n/text";
import { accountLabel } from "../../utils/accountName";
import { Toggle } from "../ui/Toggle";
import { Select } from "../ui/Select";
import { GameBadge } from "../ui/GameBadge";
import { loadFavorites, parsePlaceIdInput } from "../server-list/types";
import type { LaunchPreset, LaunchPresetView, PresetSchedule } from "../../types";
import {
  EMPTY_SCHEDULE,
  PRESET_DAY_ORDER,
  favoriteTargets,
  formatNextRun,
  matchingTarget,
  newPresetDraft,
  presetDayToJsDay,
  presetProblem,
} from "../../utils/presets";

const CUSTOM_TARGET = "custom";

/** Nome curto do dia no idioma do app (0 = domingo, como o JS). */
function weekdayShort(jsDay: number, language: string): string {
  // 11/10/2026 é um domingo: soma o dia pedido.
  const date = new Date(2026, 9, 11 + jsDay);
  try {
    return new Intl.DateTimeFormat(language, { weekday: "short" }).format(date);
  } catch {
    return ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][jsDay];
  }
}

/**
 * Presets de launch (ideia 13): a lista, com Launch num clique, e o editor.
 * Abre pela Toolbar da lista de contas ("Presets") e pelo "Save as preset" da
 * Choose Game, que já chega com as contas escolhidas. Ver docs/features/presets.md.
 */
export function PresetsDialog() {
  const t = useTr();
  const store = useStore();
  const open = store.presetsDialog !== null;
  const { visible, closing, handleClose } = useModalClose(open, store.closePresetsDialog);
  const backdropClose = useBackdropClose(handleClose);
  const [editing, setEditing] = useState<LaunchPreset | null>(null);

  // Cada abertura começa no que pediu quem abriu: lista, ou o editor com o
  // rascunho (Choose Game).
  useEffect(() => {
    if (open) setEditing(store.presetsDialog?.draft ?? null);
  }, [open, store.presetsDialog]);

  if (!visible) return null;

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${
        closing ? "animate-fade-out" : "animate-fade-in"
      }`}
      {...backdropClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("Launch presets")}
        data-testid="presets-dialog"
        className={`theme-modal-scope theme-panel theme-border border rounded-2xl shadow-2xl w-[600px] max-w-[calc(100vw-24px)] max-h-[min(720px,calc(100vh-112px))] flex flex-col overflow-hidden ${
          closing ? "animate-scale-out" : "animate-scale-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3 px-5 pt-4 pb-3 shrink-0 border-b theme-border">
          <div className="min-w-0">
            <h2 className="text-[15px] font-semibold text-[var(--panel-fg)]">
              {editing ? (editing.id ? t("Edit preset") : t("New preset")) : t("Launch presets")}
            </h2>
            <p className="text-[12px] theme-muted mt-0.5">
              {t("Save which accounts go to which game, then launch them all with one click.")}
            </p>
          </div>
          <button onClick={handleClose} className="theme-muted hover:opacity-100 p-1" aria-label={t("Close")}>
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        {editing ? (
          <PresetEditor
            key={editing.id || "new"}
            initial={editing}
            onDone={() => setEditing(null)}
          />
        ) : (
          <PresetList
            onEdit={(preset) => setEditing(preset)}
            onNew={() => setEditing(newPresetDraft([...store.selectedIds]))}
            onLaunched={handleClose}
          />
        )}
      </div>
    </div>
  );
}

// ── Lista ─────────────────────────────────────────────────────────────────────

function PresetList({
  onEdit,
  onNew,
  onLaunched,
}: {
  onEdit: (preset: LaunchPreset) => void;
  onNew: () => void;
  onLaunched: () => void;
}) {
  const t = useTr();
  const store = useStore();
  const [presets, setPresets] = useState<LaunchPresetView[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<LaunchPresetView[]>("get_launch_presets")
      .then((list) => {
        if (!alive) return;
        setPresets(Array.isArray(list) ? list : []);
        setError(null);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [store.presetsRevision]);

  return (
    <>
      <div className="flex-1 min-h-0 overflow-y-auto px-5 py-3 space-y-2">
        {error && (
          <div className="flex items-start gap-2 text-[12px] text-red-400 bg-[var(--panel-soft)] border theme-border rounded-lg px-3 py-2">
            <AlertTriangle size={13} strokeWidth={1.5} className="shrink-0 mt-[2px]" />
            <span className="break-words">{error}</span>
          </div>
        )}
        {presets && presets.length === 0 && !error && (
          <div className="text-center py-8 px-4">
            <p className="text-[13px] text-[var(--panel-fg)]">{t("No presets yet")}</p>
            <p className="text-[12px] theme-muted mt-1">
              {t("Pick the accounts and the game once; next time it's one click.")}
            </p>
          </div>
        )}
        {presets?.map((preset) => (
          <PresetRow key={preset.id} preset={preset} onEdit={() => onEdit(preset)} onLaunched={onLaunched} />
        ))}
      </div>
      <div className="shrink-0 flex items-center justify-between gap-2 px-5 py-3 border-t theme-border">
        <p className="text-[11px] theme-muted min-w-0">
          {t("Schedules run only while MultiAlt is open.")}
        </p>
        <button
          onClick={onNew}
          style={{ width: "auto" }}
          className="sidebar-btn theme-btn shrink-0 flex items-center gap-1.5 px-3"
        >
          <Plus size={13} strokeWidth={2} />
          {t("New preset")}
        </button>
      </div>
    </>
  );
}

function PresetRow({
  preset,
  onEdit,
  onLaunched,
}: {
  preset: LaunchPresetView;
  onEdit: () => void;
  onLaunched: () => void;
}) {
  const t = useTr();
  const store = useStore();
  const { i18n } = useTranslation();
  const confirmJoinOnline = useJoinOnlineWarning();
  const identity = useGameIdentity(preset.placeId);
  const [busy, setBusy] = useState(false);

  const known = new Set(store.accounts.map((a) => a.UserID));
  const present = preset.userIds.filter((id) => known.has(id));
  const gameName = identity?.name ?? preset.gameName ?? t("Place {{placeId}}", { placeId: preset.placeId });
  const server = preset.vipName
    ? t("VIP: {{name}}", { name: preset.vipName })
    : preset.jobId
      ? preset.jobId.startsWith("vip:")
        ? t("VIP server")
        : t("Specific server")
      : null;

  const now = Date.now();
  const weekday = (jsDay: number) => weekdayShort(jsDay, i18n.language);
  const schedule: string[] = [];
  if (preset.nextOpenAt) schedule.push(t("Opens {{when}}", { when: formatNextRun(preset.nextOpenAt, now, t, weekday) }));
  if (preset.nextCloseAt) schedule.push(t("Closes {{when}}", { when: formatNextRun(preset.nextCloseAt, now, t, weekday) }));

  async function launch() {
    if (busy || present.length === 0) return;
    if (!(await confirmJoinOnline(present))) return;
    setBusy(true);
    try {
      const attempt = await store.launchPreset(preset);
      if (attempt === "started") {
        store.addToast(t("Preset {{name}} launched", { name: preset.name }), "success");
        onLaunched();
      }
    } finally {
      setBusy(false);
    }
  }

  async function closeTheirs() {
    try {
      const closed = await invoke<number>("close_preset_clients", { id: preset.id });
      store.addToast(t("Closed {{count}} window(s) opened by {{name}}", { count: closed, name: preset.name }));
    } catch (e) {
      store.addToast(t("Could not close: {{error}}", { error: String(e) }), "error");
    }
  }

  return (
    <div data-testid="preset-row" className="rounded-xl border theme-border bg-[var(--panel-soft)] px-3 py-2.5">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-medium text-[var(--panel-fg)] truncate">{preset.name}</div>
          <div className="text-[12px] theme-muted truncate">
            {[
              present.length === 1 ? t("1 account") : t("{{count}} accounts", { count: present.length }),
              gameName,
              server,
            ]
              .filter(Boolean)
              .join(" · ")}
          </div>
          {present.length < preset.userIds.length && (
            <div className="text-[11px] text-amber-400 mt-0.5">
              {t("{{count}} account(s) of this preset are no longer in the app", {
                count: preset.userIds.length - present.length,
              })}
            </div>
          )}
          {schedule.length > 0 && (
            <div className="flex items-center gap-1 text-[11px] theme-muted mt-0.5">
              <Clock size={11} strokeWidth={1.75} className="shrink-0" />
              <span className="truncate">{schedule.join(" · ")}</span>
            </div>
          )}
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          <button
            onClick={onEdit}
            className="p-1.5 rounded-md theme-btn-ghost border theme-border theme-muted hover:text-[var(--panel-fg)]"
            aria-label={t("Edit {{name}}", { name: preset.name })}
            title={t("Edit")}
          >
            <Pencil size={13} strokeWidth={1.75} />
          </button>
          <button
            onClick={launch}
            disabled={busy || present.length === 0}
            style={{ width: "auto" }}
            className="sidebar-btn theme-btn flex items-center gap-1.5 px-3 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Play size={12} strokeWidth={2} />
            {busy ? t("Launching...") : t("Launch")}
          </button>
        </div>
      </div>
      {preset.openClients > 0 && (
        <div className="flex items-center justify-between gap-2 mt-2 pt-2 border-t theme-border text-[12px]">
          <span className="theme-muted">
            {t("{{count}} window(s) opened by this preset are still open", { count: preset.openClients })}
          </span>
          <button onClick={closeTheirs} className="sidebar-btn-sm shrink-0">
            {t("Close them")}
          </button>
        </div>
      )}
    </div>
  );
}

// ── Editor ────────────────────────────────────────────────────────────────────

function PresetEditor({ initial, onDone }: { initial: LaunchPreset; onDone: () => void }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const { i18n } = useTranslation();
  const [draft, setDraft] = useState<LaunchPreset>(initial);
  const [placeText, setPlaceText] = useState(initial.placeId > 0 ? String(initial.placeId) : "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const targets = useMemo(() => favoriteTargets(loadFavorites()), []);
  const [targetKey, setTargetKey] = useState<string>(
    () => matchingTarget(targets, initial.placeId, initial.jobId)?.key ?? (targets.length > 0 && initial.placeId <= 0 ? targets[0].key : CUSTOM_TARGET)
  );
  const chosen = targets.find((o) => o.key === targetKey) ?? null;
  const identity = useGameIdentity(chosen ? null : placeText);
  const schedule: PresetSchedule = draft.schedule ?? EMPTY_SCHEDULE;

  // O destino que vai ser salvo: o do favorito escolhido, ou o digitado.
  const resolved: LaunchPreset = chosen
    ? { ...draft, placeId: chosen.placeId, jobId: chosen.jobId, gameName: chosen.gameName, vipName: chosen.vipName }
    : {
        ...draft,
        placeId: parsePlaceIdInput(placeText) ?? 0,
        gameName: identity?.name ?? null,
        vipName: null,
      };

  function setSchedule(patch: Partial<PresetSchedule>) {
    setDraft((d) => ({ ...d, schedule: { ...(d.schedule ?? EMPTY_SCHEDULE), ...patch } }));
  }

  function toggleAccount(userId: number) {
    setDraft((d) => ({
      ...d,
      userIds: d.userIds.includes(userId) ? d.userIds.filter((id) => id !== userId) : [...d.userIds, userId],
    }));
  }

  function toggleDay(day: number) {
    const days = schedule.days.includes(day) ? schedule.days.filter((d) => d !== day) : [...schedule.days, day];
    setSchedule({ days: days.sort((a, b) => a - b) });
  }

  async function save() {
    const problem = presetProblem(resolved);
    if (problem) {
      setError(t(problem));
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const toSave: LaunchPreset = {
        ...resolved,
        schedule: schedule.openEnabled || schedule.closeEnabled ? schedule : null,
      };
      await invoke<LaunchPreset>("save_launch_preset", { preset: toSave });
      store.addToast(t("Preset saved"), "success");
      onDone();
    } catch (e) {
      setError(t(String(e)));
    } finally {
      setSaving(false);
    }
  }

  async function remove() {
    if (!(await confirm(t("Delete the preset {{name}}?", { name: draft.name })))) return;
    try {
      await invoke("delete_launch_preset", { id: draft.id });
      store.addToast(t("Preset deleted"));
      onDone();
    } catch (e) {
      setError(String(e));
    }
  }

  const targetOptions = [
    ...targets.map((o) => ({
      value: o.key,
      label: o.vipName ? `${o.gameName} — ${t("VIP: {{name}}", { name: o.vipName })}` : `${o.gameName} — ${t("public server")}`,
    })),
    { value: CUSTOM_TARGET, label: t("Other game (type a Place ID)") },
  ];

  const labelClass = "text-[12px] theme-label font-medium block mb-1.5";

  return (
    <>
      <div className="flex-1 min-h-0 overflow-y-auto px-5 py-3 space-y-4">
        <div>
          <label className={labelClass} htmlFor="preset-name">{t("Name")}</label>
          <input
            id="preset-name"
            value={draft.name}
            maxLength={60}
            onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
            placeholder={t("e.g. Morning farm")}
            className="sidebar-input w-full"
            autoComplete="off"
          />
        </div>

        <div>
          <div className="flex items-center justify-between mb-1.5">
            <span className="text-[12px] theme-label font-medium">
              {t("Accounts ({{count}})", { count: draft.userIds.length })}
            </span>
            <div className="flex gap-2 text-[11px]">
              <button
                className="theme-muted hover:text-[var(--panel-fg)]"
                onClick={() => setDraft((d) => ({ ...d, userIds: store.accounts.map((a) => a.UserID) }))}
              >
                {t("All")}
              </button>
              <button className="theme-muted hover:text-[var(--panel-fg)]" onClick={() => setDraft((d) => ({ ...d, userIds: [] }))}>
                {t("None")}
              </button>
            </div>
          </div>
          <div className="max-h-[150px] overflow-y-auto rounded-lg border theme-border divide-y divide-[var(--border-color)]">
            {store.accounts.map((a) => (
              <label key={a.UserID} className="flex items-center gap-2 px-2.5 py-1.5 text-[12px] cursor-pointer hover:bg-[var(--panel-soft)]">
                <input
                  type="checkbox"
                  checked={draft.userIds.includes(a.UserID)}
                  onChange={() => toggleAccount(a.UserID)}
                />
                <span className="truncate text-[var(--panel-fg)]">{accountLabel(a, store)}</span>
              </label>
            ))}
          </div>
          <p className="text-[11px] theme-muted mt-1">{t("They open one at a time, in this list's order.")}</p>
        </div>

        <div>
          <span className={labelClass}>{t("Game")}</span>
          <Select value={targetKey} options={targetOptions} onChange={setTargetKey} className="w-full" ariaLabel="Game" />
          {!chosen && (
            <div className="mt-2 grid grid-cols-2 gap-2">
              <div>
                <label className={labelClass} htmlFor="preset-place">{t("Place ID")}</label>
                <input
                  id="preset-place"
                  value={placeText}
                  onChange={(e) => setPlaceText(e.target.value)}
                  placeholder={t("Place ID or game link")}
                  className="sidebar-input w-full"
                  spellCheck={false}
                  autoComplete="off"
                />
                {identity && <GameBadge name={identity.name} iconUrl={identity.iconUrl} placeId={identity.placeId} className="mt-1" />}
              </div>
              <div>
                <label className={labelClass} htmlFor="preset-job">{t("Server (optional)")}</label>
                <input
                  id="preset-job"
                  value={draft.jobId}
                  onChange={(e) => setDraft((d) => ({ ...d, jobId: e.target.value }))}
                  placeholder={t("Job ID — empty = any public server")}
                  className="sidebar-input w-full"
                  spellCheck={false}
                  autoComplete="off"
                />
              </div>
            </div>
          )}
          {targets.length === 0 && (
            <p className="text-[11px] theme-muted mt-1">
              {t("Tip: games saved in Favorites (with their VIP servers) show up in this list.")}
            </p>
          )}
        </div>

        <Toggle
          checked={draft.arrangeGrid}
          onChange={(v) => setDraft((d) => ({ ...d, arrangeGrid: v }))}
          label="Arrange the windows in a grid when done"
          description="Same as the Arrange in grid button in Choose Game > Windows."
        />

        <div className="rounded-xl border theme-border p-3 space-y-2">
          <Toggle
            checked={schedule.openEnabled}
            onChange={(v) => setSchedule({ openEnabled: v })}
            label="Open at a set time"
            description="Launches this preset by itself at that time, while MultiAlt is open. If the app is closed then, that time is skipped."
          />
          {schedule.openEnabled && (
            <div className="flex flex-wrap items-center gap-2 pl-1">
              <input
                type="time"
                aria-label={t("Open time")}
                value={schedule.openAt}
                onChange={(e) => setSchedule({ openAt: e.target.value })}
                style={{ width: 112 }}
                className="sidebar-input tabular-nums"
              />
              <div className="flex gap-1" role="group" aria-label={t("Days")}>
                {PRESET_DAY_ORDER.map((day) => {
                  const on = schedule.days.includes(day);
                  return (
                    <button
                      key={day}
                      type="button"
                      aria-pressed={on}
                      onClick={() => toggleDay(day)}
                      className={`px-1.5 py-1 min-w-[34px] rounded-md border text-[11px] transition-colors ${
                        on
                          ? "border-[var(--accent-color)] bg-[var(--accent-soft)] text-[var(--panel-fg)]"
                          : "theme-border theme-muted"
                      }`}
                    >
                      {weekdayShort(presetDayToJsDay(day), i18n.language)}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
          <Toggle
            checked={schedule.closeEnabled}
            onChange={(v) => setSchedule({ closeEnabled: v })}
            label="Close at a set time"
            description="Every day at that time, closes only the windows this preset opened. Other windows are never touched."
          />
          {schedule.closeEnabled && (
            <div className="pl-1">
              <input
                type="time"
                aria-label={t("Close time")}
                value={schedule.closeAt}
                onChange={(e) => setSchedule({ closeAt: e.target.value })}
                style={{ width: 112 }}
                className="sidebar-input tabular-nums"
              />
            </div>
          )}
        </div>

        {error && (
          <div className="flex items-start gap-2 text-[12px] text-red-400 bg-[var(--panel-soft)] border theme-border rounded-lg px-3 py-2">
            <AlertTriangle size={13} strokeWidth={1.5} className="shrink-0 mt-[2px]" />
            <span className="break-words">{error}</span>
          </div>
        )}
      </div>

      <div className="shrink-0 flex items-center gap-2 px-5 py-3 border-t theme-border">
        {draft.id && (
          <button onClick={remove} className="flex items-center gap-1.5 text-[12px] text-red-400 hover:text-red-300 px-2 py-1">
            <Trash2 size={12} strokeWidth={1.75} />
            {t("Delete")}
          </button>
        )}
        <div className="ml-auto flex gap-2">
          <button onClick={onDone} className="sidebar-btn-sm">
            {t("Cancel")}
          </button>
          <button
            onClick={save}
            disabled={saving}
            style={{ width: "auto" }}
            className="sidebar-btn theme-btn px-4 disabled:opacity-50"
          >
            {saving ? t("Saving...") : t("Save")}
          </button>
        </div>
      </div>
    </>
  );
}
