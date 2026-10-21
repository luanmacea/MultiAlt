import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { AlertTriangle, Save } from "lucide-react";
import { errorText, pythonApi } from "./pythonApi";

interface ScriptEditorProps {
  name: string;
  /** From the folder listing: changes when the file is touched on disk. */
  modifiedMs: number;
}

/**
 * Plain editor for one script: a monospace textarea, Ctrl+S saves. The file can
 * also be edited from outside (an editor, an AI tool): when `modifiedMs`
 * changes and nothing is unsaved here the new text is loaded with a notice;
 * with unsaved edits a banner asks which side wins.
 */
export function ScriptEditor({ name, modifiedMs }: ScriptEditorProps) {
  const [source, setSource] = useState("");
  const [saved, setSaved] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [conflict, setConflict] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const loadedMs = useRef<number | null>(null);
  const savedRef = useRef("");
  const dirtyRef = useRef(false);
  const modifiedRef = useRef(modifiedMs);
  const nameRef = useRef(name);

  const dirty = source !== saved;
  useEffect(() => {
    dirtyRef.current = dirty;
    savedRef.current = saved;
    modifiedRef.current = modifiedMs;
    nameRef.current = name;
  });

  // Load when the script changes.
  useEffect(() => {
    let cancelled = false;
    loadedMs.current = null;
    setLoading(true);
    setError(null);
    setNotice(null);
    setConflict(null);
    pythonApi
      .readScript(name)
      .then((text) => {
        if (cancelled) return;
        const content = typeof text === "string" ? text : "";
        setSource(content);
        setSaved(content);
        loadedMs.current = modifiedRef.current;
      })
      .catch((e) => {
        if (!cancelled) setError(errorText(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [name]);

  // The file changed on disk.
  useEffect(() => {
    if (loadedMs.current === null || modifiedMs === loadedMs.current) return;
    let cancelled = false;
    pythonApi
      .readScript(name)
      .then((disk) => {
        if (cancelled || typeof disk !== "string") return;
        loadedMs.current = modifiedMs;
        // Same text as what we last saved or loaded: it was our own save.
        if (disk === savedRef.current) return;
        if (!dirtyRef.current) {
          setSource(disk);
          setSaved(disk);
          setNotice("Changed on disk - reloaded");
        } else {
          setConflict(disk);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [modifiedMs, name]);

  const save = useCallback(async () => {
    if (saving) return;
    setSaving(true);
    setError(null);
    const text = source;
    try {
      await pythonApi.writeScript(nameRef.current, text);
      setSaved(text);
      setConflict(null);
      setNotice(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  }, [saving, source]);

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      void save();
      return;
    }
    if (event.key === "Tab" && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      const area = event.currentTarget;
      const start = area.selectionStart;
      const end = area.selectionEnd;
      const next = `${source.slice(0, start)}    ${source.slice(end)}`;
      setSource(next);
      requestAnimationFrame(() => {
        area.selectionStart = area.selectionEnd = start + 4;
      });
    }
  }

  return (
    <section aria-label={`Editor for ${name}`} className="flex min-h-0 flex-col rounded-lg border theme-border bg-[var(--panel-soft)]">
      <div className="flex flex-wrap items-center gap-2 border-b theme-border px-3 py-1.5">
        <span className="truncate font-mono text-[12px] text-zinc-200">{name}</span>
        {dirty ? <span className="text-[11px] text-amber-300">Unsaved changes</span> : null}
        {notice ? <span className="text-[11px] text-sky-300">{notice}</span> : null}
        <button
          type="button"
          onClick={() => void save()}
          disabled={!dirty || saving}
          className="theme-btn ml-auto inline-flex items-center gap-1.5 px-2.5 py-1 text-[12px] disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Save size={12} strokeWidth={2} />
          Save
          <span className="text-[10.5px] opacity-60">Ctrl+S</span>
        </button>
      </div>

      {conflict !== null && (
        <div role="alert" className="flex flex-wrap items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-200">
          <AlertTriangle size={13} strokeWidth={2} className="shrink-0" />
          <span className="min-w-0 flex-1">This file changed on disk while you have unsaved edits here.</span>
          <button
            type="button"
            className="theme-btn-ghost px-2 py-0.5 text-[11.5px]"
            onClick={() => {
              setSource(conflict);
              setSaved(conflict);
              setConflict(null);
              setNotice("Reloaded from disk");
            }}
          >
            Load the disk version
          </button>
          <button type="button" className="theme-btn-ghost px-2 py-0.5 text-[11.5px]" onClick={() => setConflict(null)}>
            Keep my edits
          </button>
        </div>
      )}

      {error ? (
        <div role="alert" className="border-b border-red-500/30 bg-red-500/10 px-3 py-1.5 text-[12px] text-red-300">
          {error}
        </div>
      ) : null}

      <textarea
        aria-label={`Source of ${name}`}
        value={source}
        disabled={loading}
        onChange={(e) => setSource(e.target.value)}
        onKeyDown={onKeyDown}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        wrap="off"
        className="min-h-[260px] flex-1 resize-none bg-transparent p-3 font-mono text-[12px] leading-[1.55] text-zinc-200 focus:outline-none"
      />
    </section>
  );
}
