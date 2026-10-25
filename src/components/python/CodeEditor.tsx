import { useEffect, useRef } from "react";
import { Annotation, Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { HighlightStyle, bracketMatching, indentOnInput, indentUnit, syntaxHighlighting } from "@codemirror/language";
import { python } from "@codemirror/lang-python";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { tags } from "@lezer/highlight";

/** Marks a document change that came from the parent (a reload), not from the user typing. */
const fromOutside = Annotation.define<boolean>();

/**
 * Colors come from CSS variables (`--cm-*` in index.css) so the editor follows
 * the dark and light themes of the app without rebuilding the extension.
 */
const appTheme = EditorView.theme({
  "&": {
    height: "100%",
    color: "var(--panel-fg)",
    backgroundColor: "transparent",
    fontSize: "12px",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Consolas, 'Liberation Mono', monospace", lineHeight: "1.55" },
  ".cm-content": { caretColor: "var(--accent-color)", padding: "6px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent-color)" },
  ".cm-gutters": {
    backgroundColor: "transparent",
    color: "var(--cm-gutter)",
    border: "none",
    borderRight: "1px solid var(--border-color)",
  },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 10px 0 12px" },
  ".cm-activeLine": { backgroundColor: "var(--cm-active-line)" },
  ".cm-activeLineGutter": { backgroundColor: "var(--cm-active-line)", color: "var(--panel-fg)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "var(--cm-selection)",
  },
  ".cm-selectionMatch": { backgroundColor: "var(--cm-match)" },
  ".cm-searchMatch": { backgroundColor: "var(--cm-match)", outline: "1px solid var(--accent-color)" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "var(--cm-selection)" },
  "&.cm-focused .cm-matchingBracket": { backgroundColor: "var(--cm-match)", outline: "1px solid var(--border-color)" },
  ".cm-panels": {
    backgroundColor: "var(--panel-bg)",
    color: "var(--panel-fg)",
    borderColor: "var(--border-color)",
  },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--border-color)" },
  ".cm-panel.cm-search": { padding: "6px 8px", fontSize: "12px" },
  ".cm-panel.cm-search input, .cm-panel.cm-search button": {
    fontSize: "12px",
    color: "var(--panel-fg)",
    backgroundColor: "var(--textboxes-bg)",
    border: "1px solid var(--border-color)",
    borderRadius: "4px",
    margin: "0 4px 0 0",
    padding: "2px 6px",
  },
  ".cm-panel.cm-search label": { fontSize: "12px", marginRight: "6px" },
  ".cm-panel.cm-search [name=close]": { color: "var(--panel-muted)", background: "transparent", border: "none" },
});

const pythonHighlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.controlKeyword, tags.operatorKeyword, tags.moduleKeyword], color: "var(--cm-keyword)" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--cm-string)" },
  { tag: [tags.number, tags.bool, tags.null], color: "var(--cm-number)" },
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: "var(--cm-comment)", fontStyle: "italic" },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: "var(--cm-function)" },
  { tag: [tags.definition(tags.variableName), tags.className], color: "var(--cm-definition)" },
  { tag: [tags.self, tags.standard(tags.variableName)], color: "var(--cm-builtin)" },
  { tag: [tags.propertyName, tags.attributeName], color: "var(--cm-property)" },
  { tag: [tags.operator, tags.punctuation], color: "var(--cm-punctuation)" },
  { tag: tags.meta, color: "var(--cm-builtin)" },
]);

function baseExtensions(): Extension[] {
  return [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightActiveLine(),
    history(),
    drawSelection(),
    indentUnit.of("    "),
    indentOnInput(),
    bracketMatching(),
    search({ top: true }),
    highlightSelectionMatches(),
    python(),
    syntaxHighlighting(pythonHighlight),
    appTheme,
    keymap.of([...searchKeymap, ...historyKeymap, ...defaultKeymap, indentWithTab]),
  ];
}

export interface CodeEditorProps {
  /** Identifies the open file: switching it swaps the document and keeps each file's undo history. */
  docKey: string;
  value: string;
  readOnly?: boolean;
  ariaLabel: string;
  onChange: (text: string) => void;
  onSave: () => void;
}

/**
 * CodeMirror 6 editor for one file at a time. The text is owned by the parent
 * (`value`); this component pushes user edits up through `onChange` and applies
 * outside changes (a reload from disk) without reporting them back.
 */
export function CodeEditor({ docKey, value, readOnly = false, ariaLabel, onChange, onSave }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const states = useRef(new Map<string, EditorState>());
  const current = useRef(docKey);
  const mode = useRef(new Compartment());

  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  onChangeRef.current = onChange;
  onSaveRef.current = onSave;

  const modeExtension = (locked: boolean): Extension => [
    EditorState.readOnly.of(locked),
    EditorView.editable.of(!locked),
    EditorView.contentAttributes.of({ "aria-label": ariaLabel, "aria-readonly": locked ? "true" : "false" }),
  ];

  const buildState = (doc: string, locked: boolean): EditorState =>
    EditorState.create({
      doc,
      extensions: [
        baseExtensions(),
        mode.current.of(modeExtension(locked)),
        keymap.of([
          {
            key: "Mod-s",
            preventDefault: true,
            run: () => {
              onSaveRef.current();
              return true;
            },
          },
        ]),
        EditorView.updateListener.of((update) => {
          if (!update.docChanged) return;
          if (update.transactions.some((tr) => tr.annotation(fromOutside))) return;
          onChangeRef.current(update.state.doc.toString());
        }),
      ],
    });

  // Mount once.
  useEffect(() => {
    if (!host.current) return;
    const created = new EditorView({ state: buildState(value, readOnly), parent: host.current });
    view.current = created;
    const store = states.current;
    return () => {
      created.destroy();
      view.current = null;
      store.clear();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Another file: park the state of the old one and restore (or build) the new one.
  useEffect(() => {
    const editor = view.current;
    if (!editor || current.current === docKey) return;
    states.current.set(current.current, editor.state);
    current.current = docKey;
    editor.setState(states.current.get(docKey) ?? buildState(value, readOnly));
    // A parked state keeps the lock it had; bring it up to date.
    editor.dispatch({ effects: mode.current.reconfigure(modeExtension(readOnly)) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docKey]);

  // The parent changed the text (a reload from disk): replace without echoing it back.
  useEffect(() => {
    const editor = view.current;
    if (!editor) return;
    const text = editor.state.doc.toString();
    if (text === value) return;
    editor.dispatch({
      changes: { from: 0, to: text.length, insert: value },
      annotations: fromOutside.of(true),
    });
  }, [value, docKey]);

  useEffect(() => {
    view.current?.dispatch({ effects: mode.current.reconfigure(modeExtension(readOnly)) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [readOnly, ariaLabel]);

  return <div ref={host} className="h-full min-h-0 overflow-hidden" data-testid="code-editor" />;
}
