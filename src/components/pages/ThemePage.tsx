import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown } from "lucide-react";
import { useStore } from "../../store";
import { useEscapeStack } from "../../hooks/useEscapeStack";
import { PageShell } from "./PageShell";
import { useConfirm, usePrompt } from "../../hooks/usePrompt";
import type { ThemeData } from "../../types";
import { useTr } from "../../i18n/text";
import { ColorRow } from "../ui/ColorRow";
import { ToggleRow } from "../ui/ToggleRow";
import { DEFAULT_THEME, DEFAULT_FONT_MONO, DEFAULT_FONT_SANS, SYSTEM_FONT_MONO, SYSTEM_FONT_SANS, THEME_PRESETS, normalizeTheme } from "../../theme";
import { Select } from "../ui/Select";
import { MONO_GOOGLE_PRESETS, SANS_GOOGLE_PRESETS, googlePresetToSpec } from "../../fontPresets";
import type { ThemeFontSpec } from "../../types";

type Category = "Accounts" | "Buttons" | "Forms" | "Text Boxes" | "Labels" | "Fonts";
type PresetSource = "builtin" | "custom";

interface CustomThemePreset {
  id: string;
  name: string;
  theme: ThemeData;
}

interface PresetOption {
  key: string;
  source: PresetSource;
  label: string;
  theme: ThemeData;
  customId?: string;
}

const CATEGORIES: Category[] = ["Accounts", "Buttons", "Forms", "Text Boxes", "Labels", "Fonts"];
const BUILTIN_PREFIX = "builtin:";
const CUSTOM_PREFIX = "custom:";

function sortCustomPresets(presets: CustomThemePreset[]) {
  return [...presets].sort((a, b) => a.name.localeCompare(b.name));
}

function upsertCustomPreset(prev: CustomThemePreset[], next: CustomThemePreset) {
  const filtered = prev.filter((p) => p.id !== next.id);
  return sortCustomPresets([...filtered, next]);
}

function cleanPresetLabel(label: string) {
  return label.replace(/\s+\(Custom\)$/, "");
}

/**
 * Página Theme. Era um modal de 560 px com uma categoria por vez (duas a quatro
 * cores cada); na página todas as categorias ficam à vista, em cartões, e uma
 * prévia fixa ao lado mostra as superfícies do app com as cores atuais.
 *
 * A pré-visualização é do app inteiro (`applyThemePreview`), como no modal.
 * Sair da página sem salvar — pela barra lateral, pelo Escape ou por qualquer
 * outro caminho que troque `activePage` — devolve o tema salvo, que é o que o
 * Escape e o Cancel do modal faziam.
 */
export function ThemePage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const confirm = useConfirm();

  const [theme, setThemeLocal] = useState<ThemeData>({ ...DEFAULT_THEME });
  const [savedTheme, setSavedTheme] = useState<ThemeData>({ ...DEFAULT_THEME });
  const savedThemeRef = useRef<ThemeData | null>(null);
  savedThemeRef.current = savedTheme;
  const wasActiveRef = useRef(false);
  const [customPresets, setCustomPresets] = useState<CustomThemePreset[]>([]);
  const [presetId, setPresetId] = useState(`${BUILTIN_PREFIX}${THEME_PRESETS[0].id}`);
  const [presetMenuOpen, setPresetMenuOpen] = useState(false);
  const openThemeRef = useRef<ThemeData>({ ...DEFAULT_THEME });
  const presetMenuRef = useRef<HTMLDivElement>(null);
  const fontFileInputRef = useRef<HTMLInputElement>(null);
  const pendingFontKindRef = useRef<"sans" | "mono">("sans");
  const presetFileInputRef = useRef<HTMLInputElement>(null);

  const presetOptions = useMemo<PresetOption[]>(() => {
    const builtIn = THEME_PRESETS.map((preset) => ({
      key: `${BUILTIN_PREFIX}${preset.id}`,
      source: "builtin" as const,
      label: preset.label,
      theme: preset.theme,
    }));
    const custom = customPresets.map((preset) => ({
      key: `${CUSTOM_PREFIX}${preset.id}`,
      source: "custom" as const,
      label: `${preset.name} (Custom)`,
      theme: preset.theme,
      customId: preset.id,
    }));
    return [...builtIn, ...custom];
  }, [customPresets]);

  const selectedPreset = useMemo(
    () => presetOptions.find((preset) => preset.key === presetId) ?? null,
    [presetOptions, presetId]
  );
  const builtInPresetOptions = useMemo(
    () => presetOptions.filter((preset) => preset.source === "builtin"),
    [presetOptions]
  );
  const customPresetOptions = useMemo(
    () => presetOptions.filter((preset) => preset.source === "custom"),
    [presetOptions]
  );

  useEffect(() => {
    if (!active) {
      // Saiu da página: o que não foi salvo some, e o app volta ao tema salvo.
      if (wasActiveRef.current && savedThemeRef.current) {
        store.applyThemePreview(savedThemeRef.current);
      }
      wasActiveRef.current = false;
      return;
    }
    wasActiveRef.current = true;
    let cancelled = false;
    const nextTheme = normalizeTheme(store.theme ?? DEFAULT_THEME);
    openThemeRef.current = nextTheme;
    setThemeLocal(nextTheme);
    setSavedTheme(nextTheme);
    store.applyThemePreview(nextTheme);
    invoke<CustomThemePreset[]>("get_theme_presets")
      .then((presets) => {
        if (cancelled) return;
        setCustomPresets(sortCustomPresets(presets));
      })
      .catch(() => {
        if (cancelled) return;
        setCustomPresets([]);
      });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useEffect(() => {
    if (!active) return;
    setPresetMenuOpen(false);
  }, [active]);

  // Com a lista de presets aberta, o Escape fecha a lista — e não a página.
  useEscapeStack(active && presetMenuOpen, () => setPresetMenuOpen(false));

  useEffect(() => {
    if (!presetMenuOpen) return;
    function onMouseDown(e: MouseEvent) {
      if (!presetMenuRef.current) return;
      if (!presetMenuRef.current.contains(e.target as Node)) {
        setPresetMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", onMouseDown);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
    };
  }, [presetMenuOpen]);

  useEffect(() => {
    if (presetOptions.length === 0) return;
    if (presetOptions.some((preset) => preset.key === presetId)) return;
    const fallback = presetOptions[0]?.key;
    if (fallback && presetOptions.some((preset) => preset.key === fallback)) {
      setPresetId(fallback);
    }
  }, [presetOptions, presetId]);

  const applyLive = useCallback(
    (updated: ThemeData) => {
      store.applyThemePreview(updated);
    },
    [store.applyThemePreview]
  );

  if (!active) return null;

  function update(partial: Partial<ThemeData>) {
    setThemeLocal((prev) => {
      const next = { ...prev, ...partial };
      applyLive(next);
      return next;
    });
  }

  function isFontSpecEqual(a: ThemeFontSpec | undefined, b: ThemeFontSpec) {
    if (!a) return false;
    return JSON.stringify(a) === JSON.stringify(b);
  }

  function fontSpecToSelectValue(spec: ThemeFontSpec | undefined, fallbackDefault: ThemeFontSpec) {
    const s = spec ?? fallbackDefault;
    if (isFontSpecEqual(s, fallbackDefault)) return "default";
    if (s.source === "system") return "system";
    if (s.source === "google") return `google:${s.family}`;
    if (s.source === "local" && s.local?.file) return `local:${s.local.file}`;
    return "default";
  }

  function buildFontOptions(kind: "sans" | "mono", current: ThemeFontSpec | undefined, fallbackDefault: ThemeFontSpec) {
    const options: Array<{ value: string; label: string }> = [
      {
        value: "default",
        label: kind === "sans" ? t("Default (Outfit)") : t("Default (JetBrains Mono)"),
      },
      { value: "system", label: kind === "sans" ? "System UI" : "System Mono" },
    ];

    const presets = kind === "sans" ? SANS_GOOGLE_PRESETS : MONO_GOOGLE_PRESETS;
    for (const p of presets) options.push({ value: `google:${p.family}`, label: p.label });

    if (current?.source === "local" && current.local?.file) {
      options.push({ value: `local:${current.local.file}`, label: `Local: ${current.family || current.local.file}` });
    }

    const selectedValue = fontSpecToSelectValue(current, fallbackDefault);
    if (!options.some((o) => o.value === selectedValue)) {
      options.unshift({ value: selectedValue, label: selectedValue });
    }

    return { options, selectedValue };
  }

  function importLocalFont(kind: "sans" | "mono") {
    pendingFontKindRef.current = kind;
    fontFileInputRef.current?.click();
  }

  async function handleFontFileSelected(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    const kind = pendingFontKindRef.current;
    try {
      const fileData = Array.from(new Uint8Array(await file.arrayBuffer()));
      const result = await invoke<{ file: string; suggested_family: string }>("import_theme_font_bytes", {
        fileName: file.name,
        fileData,
      });
      const family = result.suggested_family || "Custom Font";
      const fallbacks = kind === "sans" ? DEFAULT_FONT_SANS.fallbacks : DEFAULT_FONT_MONO.fallbacks;
      const next: ThemeFontSpec = {
        source: "local",
        family,
        fallbacks,
        local: { file: result.file, weight: 400, style: "normal" },
      };
      update(kind === "sans" ? { font_sans: next } : { font_mono: next });
      store.addToast(`Imported font: ${family}`);
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  /** Volta ao tema salvo sem sair da página. */
  function handleDiscard() {
    const saved = savedTheme ?? openThemeRef.current;
    setThemeLocal(saved);
    store.applyThemePreview(saved);
  }

  async function handleSave() {
    try {
      const normalized = normalizeTheme(theme);
      await store.saveTheme(normalized);
      setSavedTheme(normalized);
      setThemeLocal(normalized);
      store.addToast(t("Theme saved"));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  function handleReset() {
    const defaults = normalizeTheme(DEFAULT_THEME);
    setThemeLocal(defaults);
    applyLive(defaults);
  }

  function applyPresetByKey(key: string) {
    const selected = presetOptions.find((preset) => preset.key === key);
    if (!selected) return;
    const next = normalizeTheme(selected.theme);
    setThemeLocal(next);
    applyLive(next);
  }

  function handlePresetSelection(nextKey: string) {
    setPresetId(nextKey);
    applyPresetByKey(nextKey);
    setPresetMenuOpen(false);
  }

  async function handleSavePreset() {
    const suggestedName = cleanPresetLabel(selectedPreset?.label || t("My Preset"));
    const name = await prompt(t("Preset name:"), suggestedName);
    if (!name?.trim()) return;
    try {
      const preset = await invoke<CustomThemePreset>("save_theme_preset", {
        name: name.trim(),
        theme: normalizeTheme(theme),
      });
      setCustomPresets((prev) => upsertCustomPreset(prev, preset));
      setPresetId(`${CUSTOM_PREFIX}${preset.id}`);
      store.addToast(t("Preset saved: {{name}}", { name: preset.name }));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  async function handleDeletePreset() {
    if (!selectedPreset?.customId) return;
    const name = cleanPresetLabel(selectedPreset.label);
    const ok = await confirm(t("Delete preset \"{{name}}\"?", { name }), true);
    if (!ok) return;
    try {
      await invoke("delete_theme_preset", { presetId: selectedPreset.customId });
      setCustomPresets((prev) => prev.filter((preset) => preset.id !== selectedPreset.customId));
      setPresetId(`${BUILTIN_PREFIX}${THEME_PRESETS[0].id}`);
      store.addToast(t("Preset deleted: {{name}}", { name }));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  function handleImportPresetFile() {
    presetFileInputRef.current?.click();
  }

  async function handlePresetFileSelected(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    try {
      const fileData = Array.from(new Uint8Array(await file.arrayBuffer()));
      const preset = await invoke<CustomThemePreset>("import_theme_preset_bytes", {
        fileName: file.name,
        fileData,
      });
      setCustomPresets((prev) => upsertCustomPreset(prev, preset));
      const key = `${CUSTOM_PREFIX}${preset.id}`;
      setPresetId(key);
      const next = normalizeTheme(preset.theme);
      setThemeLocal(next);
      applyLive(next);
      store.addToast(t("Imported preset: {{name}}", { name: preset.name }));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  async function handleExportPresetFile() {
    const suggestedName = cleanPresetLabel(selectedPreset?.label || t("theme-preset"));
    const name = await prompt(t("Export file name (no extension):"), suggestedName);
    if (name === null) return;
    const exportName = name.trim() || suggestedName;
    try {
      const path = await invoke<string>("export_theme_preset_file", {
        name: exportName,
        theme: normalizeTheme(theme),
      });
      store.addToast(t("Exported preset to {{path}}", { path }));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  function cycleButtonStyle() {
    const styles = ["Flat", "Popup", "Standard"];
    const idx = styles.indexOf(theme.button_style);
    const next = styles[(idx + 1) % styles.length];
    update({ button_style: next });
  }

  function renderControls(category: Category) {
    switch (category) {
      case "Accounts":
        return (
          <>
            <ColorRow label="Background" value={theme.accounts_background} onChange={(v) => update({ accounts_background: v })} />
            <ColorRow label="Foreground" value={theme.accounts_foreground} onChange={(v) => update({ accounts_foreground: v })} />
            <ToggleRow label="Light Avatars" checked={theme.light_images} onChange={(v) => update({ light_images: v })} />
            <ToggleRow label="Show Headers" checked={theme.show_headers} onChange={(v) => update({ show_headers: v })} />
          </>
        );
      case "Buttons":
        return (
          <>
            <ColorRow label="Background" value={theme.buttons_background} onChange={(v) => update({ buttons_background: v })} />
            <ColorRow label="Foreground" value={theme.buttons_foreground} onChange={(v) => update({ buttons_foreground: v })} />
            <ColorRow label="Border" value={theme.buttons_border} onChange={(v) => update({ buttons_border: v })} />
            <div className="mt-2" />
            <ColorRow label="Toggle On" value={theme.toggle_on_background || "#0EA5E9"} onChange={(v) => update({ toggle_on_background: v })} />
            <ColorRow label="Toggle Off" value={theme.toggle_off_background || "#3F3F46"} onChange={(v) => update({ toggle_off_background: v })} />
            <ColorRow label="Toggle Knob" value={theme.toggle_knob_background || "#FFFFFF"} onChange={(v) => update({ toggle_knob_background: v })} />
            <div className="flex items-center justify-between py-1.5">
              <span className="text-xs text-[var(--panel-fg)]">{t("Button Style")}</span>
                <button
                  onClick={cycleButtonStyle}
                  className="theme-btn px-2.5 py-1 rounded text-[11px]"
                >
                  {t(theme.button_style)}
                </button>
            </div>
          </>
        );
      case "Forms":
        return (
          <>
            <ColorRow label="Background" value={theme.forms_background} onChange={(v) => update({ forms_background: v })} />
            <ColorRow label="Foreground" value={theme.forms_foreground} onChange={(v) => update({ forms_foreground: v })} />
            <ToggleRow label="Dark Top Bar" checked={theme.dark_top_bar} onChange={(v) => update({ dark_top_bar: v })} />
          </>
        );
      case "Text Boxes":
        return (
          <>
            <ColorRow label="Background" value={theme.textboxes_background} onChange={(v) => update({ textboxes_background: v })} />
            <ColorRow label="Foreground" value={theme.textboxes_foreground} onChange={(v) => update({ textboxes_foreground: v })} />
            <ColorRow label="Border" value={theme.textboxes_border} onChange={(v) => update({ textboxes_border: v })} />
          </>
        );
      case "Labels":
        return (
          <>
            <ColorRow label="Background" value={theme.label_background} onChange={(v) => update({ label_background: v })} />
            <ColorRow label="Foreground" value={theme.label_foreground} onChange={(v) => update({ label_foreground: v })} />
            <ToggleRow label="Transparent BG" checked={theme.label_transparent} onChange={(v) => update({ label_transparent: v })} />
          </>
        );
      case "Fonts": {
        const sans = theme.font_sans;
        const mono = theme.font_mono;
        const sansSelect = buildFontOptions("sans", sans, DEFAULT_FONT_SANS);
        const monoSelect = buildFontOptions("mono", mono, DEFAULT_FONT_MONO);

        function applySelect(kind: "sans" | "mono", value: string) {
          if (value === "default") {
            update(kind === "sans" ? { font_sans: { ...DEFAULT_FONT_SANS } } : { font_mono: { ...DEFAULT_FONT_MONO } });
            return;
          }
          if (value === "system") {
            update(kind === "sans" ? { font_sans: { ...SYSTEM_FONT_SANS } } : { font_mono: { ...SYSTEM_FONT_MONO } });
            return;
          }
          if (value.startsWith("google:")) {
            const family = value.slice("google:".length);
            const presets = kind === "sans" ? SANS_GOOGLE_PRESETS : MONO_GOOGLE_PRESETS;
            const preset = presets.find((p) => p.family === family);
            if (!preset) return;
            update(kind === "sans" ? { font_sans: googlePresetToSpec(preset) } : { font_mono: googlePresetToSpec(preset) });
            return;
          }
          if (value.startsWith("local:")) {
            // Selecting an existing local font is a no-op (we already store the spec).
            return;
          }
        }

        return (
          <div className="space-y-4">
            <input
              ref={fontFileInputRef}
              type="file"
              accept=".ttf,.otf,.woff,.woff2"
              className="hidden"
              data-testid="import-font-file-input"
              onChange={handleFontFileSelected}
            />
            <div className="flex items-center justify-between gap-3">
              <div className="text-xs text-[var(--panel-fg)] shrink-0 w-20">Sans</div>
              <div className="flex-1">
                <Select
                  value={sansSelect.selectedValue}
                  options={sansSelect.options}
                  onChange={(v) => applySelect("sans", v)}
                  className="w-full"
                />
              </div>
              <button onClick={() => importLocalFont("sans")} className="theme-btn px-2.5 py-1.5 text-xs font-medium shrink-0">
                {t("Import Local")}
              </button>
            </div>

            <div className="flex items-center justify-between gap-3">
              <div className="text-xs text-[var(--panel-fg)] shrink-0 w-20">Mono</div>
              <div className="flex-1">
                <Select
                  value={monoSelect.selectedValue}
                  options={monoSelect.options}
                  onChange={(v) => applySelect("mono", v)}
                  className="w-full"
                />
              </div>
              <button onClick={() => importLocalFont("mono")} className="theme-btn px-2.5 py-1.5 text-xs font-medium shrink-0">
                {t("Import Local")}
              </button>
            </div>

            <div className="text-[12px] theme-muted">
              {/* Uma frase so: com os nomes de arquivo em <span> separados, cada
                  pedaco ia para o catalogo sozinho e nao dava para traduzir. */}
              {t(
                "Tip: exporting uses .ram-theme.json unless you use local font files; local fonts export as a .ram-theme.zip bundle."
              )}
            </div>
          </div>
        );
      }
    }
  }

  const dirty = JSON.stringify(normalizeTheme(theme)) !== JSON.stringify(normalizeTheme(savedTheme));

  return (
    <PageShell
      title={t("Theme")}
      description={t(
        "Colors and fonts apply to the whole app while you edit. Save to keep them; leaving the page without saving puts the saved theme back."
      )}
      onLeave={onLeave}
      dataTour="theme-page"
      bodyClassName="overflow-y-auto"
      actions={
        <>
          <button onClick={handleReset} className="theme-btn-ghost px-3 py-1.5 text-xs">
            {t("Reset to Defaults")}
          </button>
          <button
            onClick={handleDiscard}
            disabled={!dirty}
            className="theme-btn px-3 py-1.5 text-xs font-medium disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t("Discard changes")}
          </button>
          <button
            onClick={handleSave}
            className="theme-btn theme-accent theme-accent-bg theme-accent-border px-4 py-1.5 text-xs font-semibold"
          >
            {t("Save")}
          </button>
        </>
      }
    >
      <div className="grid gap-6 px-6 py-5 lg:grid-cols-[minmax(0,1fr)_272px] lg:items-start">
        <div className="min-w-0 space-y-5">
          <section aria-labelledby="theme-presets-heading">
            <h2 id="theme-presets-heading" className="mb-2 text-[12.5px] font-semibold text-[var(--panel-fg)]">
              {t("Presets")}
            </h2>
            <div className="flex flex-wrap items-center gap-2">
              <div ref={presetMenuRef} className="relative w-full sm:w-64">
                <button
                  onClick={() => setPresetMenuOpen((prev) => !prev)}
                  className="theme-input w-full px-3 py-1.5 rounded-lg text-xs flex items-center justify-between transition-all duration-150 hover:brightness-110"
                  aria-haspopup="listbox"
                  aria-expanded={presetMenuOpen}
                >
                  <span className="truncate text-[var(--panel-fg)]">
                    {cleanPresetLabel(selectedPreset?.label ?? t("Select preset"))}
                  </span>
                  <ChevronDown size={12} strokeWidth={2} className={`theme-muted transition-transform duration-150 ${presetMenuOpen ? "rotate-180" : ""}`} />
                </button>
                <div
                  className={`absolute left-0 right-0 top-full mt-2 z-30 transition-all duration-150 origin-top ${
                    presetMenuOpen
                      ? "opacity-100 scale-100 pointer-events-auto"
                      : "opacity-0 scale-95 pointer-events-none"
                  }`}
                >
                  <div className="theme-panel theme-border border rounded-xl shadow-2xl max-h-72 overflow-y-auto p-1.5">
                    <div className="px-2 py-1 text-[11px] theme-muted font-semibold">{t("Built-in")}</div>
                    {builtInPresetOptions.map((preset) => {
                      const selected = preset.key === presetId;
                      return (
                        <button
                          key={preset.key}
                          onClick={() => handlePresetSelection(preset.key)}
                          className={`w-full text-left px-2.5 py-1.5 rounded-lg text-xs border transition-all duration-150 ${
                            selected
                              ? "theme-accent theme-accent-bg theme-accent-border"
                              : "border-transparent text-[var(--panel-fg)] hover:bg-[var(--panel-soft)]"
                          }`}
                        >
                          {t(preset.label)}
                        </button>
                      );
                    })}
                    <div className="mx-2 my-1 border-t theme-border" />
                    <div className="px-2 py-1 text-[11px] theme-muted font-semibold">{t("Custom")}</div>
                    {customPresetOptions.length > 0 ? (
                      customPresetOptions.map((preset) => {
                        const selected = preset.key === presetId;
                        return (
                          <button
                            key={preset.key}
                            onClick={() => handlePresetSelection(preset.key)}
                            className={`w-full text-left px-2.5 py-1.5 rounded-lg text-xs border transition-all duration-150 ${
                              selected
                                ? "theme-accent theme-accent-bg theme-accent-border"
                                : "border-transparent text-[var(--panel-fg)] hover:bg-[var(--panel-soft)]"
                            }`}
                          >
                            {t(cleanPresetLabel(preset.label))}
                          </button>
                        );
                      })
                    ) : (
                      <div className="px-2.5 py-2 text-[12px] theme-muted">{t("No custom presets yet")}</div>
                    )}
                  </div>
                </div>
              </div>
              <button onClick={handleSavePreset} className="theme-btn px-3 py-1.5 text-xs font-medium">
                {t("Save Preset")}
              </button>
              <input
                ref={presetFileInputRef}
                type="file"
                accept=".json,.ram-theme.json,.zip,.ram-theme.zip"
                className="hidden"
                data-testid="import-preset-file-input"
                onChange={handlePresetFileSelected}
              />
              <button onClick={handleImportPresetFile} className="theme-btn px-3 py-1.5 text-xs font-medium">
                {t("Import File")}
              </button>
              <button onClick={handleExportPresetFile} className="theme-btn px-3 py-1.5 text-xs font-medium">
                {t("Export File")}
              </button>
              <button
                onClick={handleDeletePreset}
                disabled={!selectedPreset?.customId}
                className="theme-btn px-3 py-1.5 text-xs font-medium disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {t("Delete Preset")}
              </button>
            </div>
          </section>

          <div className="grid gap-4 xl:grid-cols-2 xl:items-start">
            {CATEGORIES.map((cat) => (
              <section
                key={cat}
                aria-labelledby={`theme-cat-${cat}`}
                className={`rounded-xl border theme-border px-4 pt-3 pb-2 ${cat === "Fonts" ? "xl:col-span-2" : ""}`}
              >
                <h2 id={`theme-cat-${cat}`} className="mb-1 text-[12.5px] font-semibold text-[var(--panel-fg)]">
                  {t(cat)}
                </h2>
                {renderControls(cat)}
              </section>
            ))}
          </div>
        </div>

        <ThemePreview />
      </div>
    </PageShell>
  );
}

/**
 * Amostra das superfícies que o tema pinta — barra de título, lista de contas,
 * formulário com campo, botão e toggle — usando as mesmas classes do app. É só
 * para olhar: nada aqui recebe foco nem clique.
 */
function ThemePreview() {
  const t = useTr();
  const rows = ["Builderman_01", "NightOwl", "Pixelmancer"];
  return (
    <aside aria-label={t("Preview")} className="lg:sticky lg:top-0">
      <h2 className="mb-2 text-[12.5px] font-semibold text-[var(--panel-fg)]">{t("Preview")}</h2>
      <div aria-hidden="true" className="rounded-xl border theme-border overflow-hidden shadow-xl select-none">
        <div className="theme-titlebar flex items-center justify-between h-7 px-3 text-[11px]">
          <span className="font-semibold">MultiAlt</span>
          <span className="flex items-center gap-2 opacity-60">
            <span className="w-2 h-px bg-current" />
            <span className="w-2 h-2 border border-current" />
          </span>
        </div>
        <div className="theme-panel py-1">
          {rows.map((name, index) => (
            <div
              key={name}
              className={`flex items-center gap-2.5 px-3 py-1.5 ${index === 1 ? "theme-row-selected" : ""}`}
            >
              <span className="theme-avatar w-6 h-6 rounded-full border theme-border bg-[var(--panel-soft)] flex items-center justify-center text-[11px] text-[var(--panel-fg)]">
                {name[0]}
              </span>
              <span className="text-[12px] font-medium text-[var(--panel-fg)] truncate">{name}</span>
            </div>
          ))}
        </div>
        <div className="theme-surface border-t theme-border p-3 space-y-2.5">
          <div>
            <span className="theme-label theme-label-bg text-[11px]">{t("Place ID")}</span>
            <div className="theme-input mt-1 rounded-md px-2 py-1 text-[12px] font-mono">606849621</div>
          </div>
          <div className="flex items-center justify-between">
            <span className="theme-btn px-3 py-1 text-[12px]">{t("Join")}</span>
            <span className="flex items-center gap-1.5">
              <span className="w-8 h-[18px] rounded-full relative border bg-[var(--toggle-off-bg)] border-[var(--toggle-off-bg)]">
                <span className="w-3.5 h-3.5 rounded-full bg-[var(--toggle-knob-bg)] absolute top-[1px] left-[1px]" />
              </span>
              <span className="w-8 h-[18px] rounded-full relative border bg-[var(--toggle-on-bg)] border-[var(--toggle-on-bg)]">
                <span className="w-3.5 h-3.5 rounded-full bg-[var(--toggle-knob-bg)] absolute top-[1px] left-[15px]" />
              </span>
            </span>
          </div>
        </div>
      </div>
    </aside>
  );
}

