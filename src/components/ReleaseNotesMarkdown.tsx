import { useMemo } from "react";
import { AlertTriangle, Info } from "lucide-react";
import { REPO_URL } from "../repo";

/**
 * Desenha as notas de release (o markdown que o GitHub guarda) com os blocos
 * que elas usam: lista, título, aviso `> [!WARNING]`/`> [!NOTE]`, código,
 * links, `@autor` e `#PR`. Saiu da janela de atualização para ser dividido com
 * a página de novidades; o texto em si é preparado em
 * [releaseNotes.ts](../releaseNotes.ts).
 */
export function renderReleaseNotes(src: string): React.ReactNode[] {
  const lines = src.split(/\r?\n/);
  const htmlTag = /<[a-z][^>]*>/i;
  const repoBaseUrl = REPO_URL;
  const nodes: React.ReactNode[] = [];
  let key = 0;

  function unescapeMarkdown(text: string): string {
    return text.replace(/\\([\\`*_{}\[\]()#+\-.!])/g, "$1");
  }

  function trimUrlToken(value: string): { href: string; suffix: string } {
    let href = value;
    let suffix = "";

    while (href.length > 0) {
      const last = href[href.length - 1];
      if (!/[),.;!?]/.test(last)) break;
      if (last === ")") {
        const openCount = (href.match(/\(/g) || []).length;
        const closeCount = (href.match(/\)/g) || []).length;
        if (closeCount <= openCount) break;
      }
      suffix = last + suffix;
      href = href.slice(0, -1);
    }

    return { href, suffix };
  }

  function renderLink(href: string, label?: string): React.ReactNode {
    return (
      <a
        key={key++}
        href={href}
        target="_blank"
        rel="noreferrer"
        className="text-sky-300 hover:text-sky-200 underline decoration-sky-500/50 underline-offset-2 [overflow-wrap:anywhere]"
      >
        {label ?? href}
      </a>
    );
  }

  function inlinePass(text: string): React.ReactNode[] {
    const parts: React.ReactNode[] = [];
    let cursor = 0;
    const re = /`([^`\n]+)`|\*\*([^*\n]+)\*\*|__([^_\n]+)__|\*([^*\n]+)\*|_([^_\n]+)_|\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)|<((?:https?:\/\/)[^>\s]+)>|(https?:\/\/[^\s<]+)|(@[a-zA-Z0-9-]{1,39})|#(\d+)/g;
    let match: RegExpExecArray | null;

    const appendText = (value: string) => {
      const plain = unescapeMarkdown(value);
      if (plain) parts.push(plain);
    };

    while ((match = re.exec(text)) !== null) {
      if (match.index > cursor) appendText(text.slice(cursor, match.index));

      if (match[1]) {
        parts.push(
          <code key={key++} className="font-mono text-[12px] px-1 py-0.5 rounded bg-black/30 border border-white/10 text-zinc-200">
            {match[1]}
          </code>
        );
      } else if (match[2] || match[3]) {
        const value = match[2] || match[3] || "";
        parts.push(<strong key={key++} className="font-semibold text-[var(--panel-fg)]">{value}</strong>);
      } else if (match[4] || match[5]) {
        const value = match[4] || match[5] || "";
        parts.push(<em key={key++} className="italic text-[var(--panel-fg)]/95">{value}</em>);
      } else if (match[6] && match[7]) {
        parts.push(renderLink(match[7], match[6]));
      } else if (match[8]) {
        parts.push(renderLink(match[8]));
      } else if (match[9]) {
        const { href, suffix } = trimUrlToken(match[9]);
        if (href) parts.push(renderLink(href));
        if (suffix) parts.push(suffix);
      } else if (match[10]) {
        const login = match[10].slice(1);
        parts.push(renderLink(`https://github.com/${login}`, match[10]));
      } else if (match[11]) {
        parts.push(renderLink(`${repoBaseUrl}/pull/${match[11]}`, `#${match[11]}`));
      }

      cursor = match.index + match[0].length;
    }

    if (cursor < text.length) appendText(text.slice(cursor));
    return parts;
  }

  function extractAvatarAnchors(line: string): Array<{ href: string; src: string; alt: string }> {
    const anchors = line.match(/<a\b[^>]*>[\s\S]*?<\/a>/gi);
    if (!anchors) return [];
    const items: Array<{ href: string; src: string; alt: string }> = [];

    for (const anchor of anchors) {
      const hrefMatch = anchor.match(/href=(["'])(.*?)\1/i);
      const imgTagMatch = anchor.match(/<img\b[^>]*>/i);
      if (!hrefMatch || !imgTagMatch) continue;

      const srcMatch = imgTagMatch[0].match(/src=(["'])(.*?)\1/i);
      if (!srcMatch) continue;

      const altMatch = imgTagMatch[0].match(/alt=(["'])(.*?)\1/i);
      items.push({
        href: hrefMatch[2],
        src: srcMatch[2],
        alt: altMatch?.[2] || "",
      });
    }

    return items;
  }

  function isBlockStart(line: string): boolean {
    return /^\s*(`{3,}|~{3,})/.test(line)
      || /^>\s?/.test(line)
      || /^(#{1,6})\s+/.test(line)
      || /^[-*_]{3,}\s*$/.test(line)
      || /^(\s*)\\?[-*+]\s+/.test(line)
      || /^(\s*)\d+[.)]\s+/.test(line)
      || /^([A-Za-z][A-Za-z ]{1,40}):\s+/.test(line)
      || extractAvatarAnchors(line).length > 0;
  }

  for (let i = 0; i < lines.length;) {
    const line = lines[i].trimEnd();

    if (!line.trim()) {
      i += 1;
      continue;
    }

    const fenceStart = line.match(/^\s*(`{3,}|~{3,})\s*([a-zA-Z0-9_-]+)?\s*$/);
    if (fenceStart) {
      const marker = fenceStart[1];
      const lang = (fenceStart[2] || "").toLowerCase();
      const codeLines: string[] = [];
      i += 1;

      while (i < lines.length) {
        const nextLine = lines[i];
        const isFenceEnd = new RegExp(`^\\s*${marker}\\s*$`).test(nextLine);
        if (isFenceEnd) {
          i += 1;
          break;
        }
        codeLines.push(nextLine);
        i += 1;
      }

      const code = codeLines.join("\n");
      if (code.trim()) {
        nodes.push(
          <div key={key++} className="my-2 rounded-lg border theme-border bg-black/25 overflow-hidden">
            {lang ? (
              <div className="px-2.5 py-1 text-[11px] uppercase tracking-wide text-zinc-400/80 border-b theme-border bg-black/20">
                {lang}
              </div>
            ) : null}
            <pre className="px-3 py-2.5 text-[12px] leading-relaxed overflow-x-auto">
              <code className="font-mono whitespace-pre text-zinc-200">{code}</code>
            </pre>
          </div>
        );
      }

      continue;
    }

    if (/^>\s?/.test(line)) {
      const quoteLines: string[] = [];
      while (i < lines.length) {
        const quoteMatch = lines[i].trimEnd().match(/^>\s?(.*)$/);
        if (!quoteMatch) break;
        quoteLines.push(quoteMatch[1]);
        i += 1;
      }

      const calloutMatch = quoteLines[0]?.match(/^\[!([A-Z]+)\]\s*$/i);
      const styleByType = {
        WARNING: {
          label: "Warning",
          wrapper: "border-amber-500/70 bg-amber-500/10",
          labelColor: "text-amber-300",
          icon: AlertTriangle,
        },
        NOTE: {
          label: "Note",
          wrapper: "border-sky-500/60 bg-sky-500/10",
          labelColor: "text-sky-300",
          icon: Info,
        },
      } as const;

      let type = "";
      let contentLines = quoteLines;
      if (calloutMatch) {
        type = calloutMatch[1].toUpperCase();
        contentLines = quoteLines.slice(1);
      }

      while (contentLines.length > 0 && !contentLines[0].trim()) {
        contentLines.shift();
      }
      while (contentLines.length > 0 && !contentLines[contentLines.length - 1].trim()) {
        contentLines.pop();
      }

      let paragraphs = contentLines
        .join("\n")
        .split(/\n{2,}/)
        .map((part) => part.split("\n").map((entry) => entry.trim()).join(" ").trim())
        .filter(Boolean);

      if (type && paragraphs.length === 0) {
        const fallbackLines: string[] = [];
        while (i < lines.length) {
          const candidate = lines[i].trimEnd();
          if (!candidate.trim()) break;
          if (isBlockStart(candidate)) break;
          if (htmlTag.test(candidate)) break;
          fallbackLines.push(candidate.trim());
          i += 1;
        }
        if (fallbackLines.length > 0) {
          paragraphs = [fallbackLines.join(" ")];
        }
      }

      if (type && type in styleByType) {
        const style = styleByType[type as keyof typeof styleByType];
        const Icon = style.icon;
        nodes.push(
          <div key={key++} className={`my-2 rounded-md border-l-2 px-3 py-2 ${style.wrapper}`}>
            <div className={`flex items-center gap-1.5 text-[12px] font-semibold ${style.labelColor}`}>
              <Icon size={13} strokeWidth={2.1} />
              <span>{style.label}</span>
            </div>
            <div className="mt-1.5 space-y-1.5 text-[12px] text-[var(--panel-fg)]/95">
              {paragraphs.map((paragraph) => (
                <p key={key++}>{inlinePass(paragraph)}</p>
              ))}
            </div>
          </div>
        );
      } else if (paragraphs.length > 0) {
        nodes.push(
          <blockquote key={key++} className="my-2 border-l-2 border-[var(--border-color)]/80 pl-3 text-[var(--panel-fg)]/90 space-y-1.5">
            {paragraphs.map((paragraph) => (
              <p key={key++}>{inlinePass(paragraph)}</p>
            ))}
          </blockquote>
        );
      }

      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      const level = heading[1].length;
      const text = heading[2].trim();
      const headingClass = level === 1
        ? "text-[15px] mt-3 mb-2"
        : level === 2
          ? "text-[14px] mt-3 mb-2 pb-1 border-b border-[var(--border-color)]/75"
          : "text-[12.5px] mt-2.5 mb-1.5";

      nodes.push(
        <div key={key++} className={`${headingClass} font-semibold text-[var(--panel-fg)]`}>
          {inlinePass(text)}
        </div>
      );
      i += 1;
      continue;
    }

    if (/^[-*_]{3,}\s*$/.test(line)) {
      nodes.push(<div key={key++} className="h-px bg-[var(--border-color)]/70 my-2" />);
      i += 1;
      continue;
    }

    const unorderedMatch = line.match(/^(\s*)\\?[-*+]\s+(.+)$/);
    if (unorderedMatch) {
      const items: Array<{ indent: number; text: string }> = [];

      while (i < lines.length) {
        const match = lines[i].trimEnd().match(/^(\s*)\\?[-*+]\s+(.+)$/);
        if (!match) break;
        const indent = Math.min(4, Math.floor(match[1].replace(/\t/g, "  ").length / 2));
        const task = match[2].match(/^\[( |x|X)\]\s+(.+)$/);
        items.push({
          indent,
          text: task ? task[2] : match[2],
        });
        i += 1;
      }

      nodes.push(
        <ul key={key++} className="my-1.5 ml-4 space-y-1 list-disc marker:text-zinc-300/90">
          {items.map((item) => (
            <li key={key++} style={{ marginLeft: `${item.indent * 10}px` }} className="pl-0.5">
              {inlinePass(item.text)}
            </li>
          ))}
        </ul>
      );
      continue;
    }

    const orderedMatch = line.match(/^(\s*)(\d+)[.)]\s+(.+)$/);
    if (orderedMatch) {
      const items: Array<{ indent: number; text: string }> = [];

      while (i < lines.length) {
        const match = lines[i].trimEnd().match(/^(\s*)(\d+)[.)]\s+(.+)$/);
        if (!match) break;
        const indent = Math.min(4, Math.floor(match[1].replace(/\t/g, "  ").length / 2));
        items.push({ indent, text: match[3] });
        i += 1;
      }

      nodes.push(
        <ol key={key++} className="my-1.5 ml-4 space-y-1 list-decimal marker:text-zinc-300/90">
          {items.map((item) => (
            <li key={key++} style={{ marginLeft: `${item.indent * 10}px` }} className="pl-0.5">
              {inlinePass(item.text)}
            </li>
          ))}
        </ol>
      );
      continue;
    }

    const metadataLine = line.match(/^([A-Za-z][A-Za-z ]{1,40}):\s+(.+)$/);
    if (metadataLine) {
      const label = metadataLine[1].trim();
      const value = metadataLine[2].trim();
      const normalizedLabel = label.toLowerCase();
      const shaLike = /^[0-9a-f]{6,40}$/i.test(value);

      let valueNode: React.ReactNode = inlinePass(value);
      if (normalizedLabel === "release commit" && shaLike) {
        valueNode = (
          <code className="font-mono text-[12px] px-1 py-0.5 rounded bg-black/30 border border-white/10 text-zinc-200">
            {value}
          </code>
        );
      }

      nodes.push(
        <p key={key++} className="my-1 [overflow-wrap:anywhere]">
          <span className="font-semibold text-[var(--panel-fg)]">{label}:</span>{" "}
          <span className="text-[var(--panel-fg)]/95">{valueNode}</span>
        </p>
      );
      i += 1;
      continue;
    }

    const avatarAnchors = extractAvatarAnchors(line);
    if (avatarAnchors.length > 0) {
      nodes.push(
        <div key={key++} className="my-2 flex flex-wrap gap-2">
          {avatarAnchors.map((anchor) => {
            const fallbackLabel = anchor.href.split("/").pop() || "contributor";
            const alt = anchor.alt || `@${fallbackLabel}`;
            return (
              <a
                key={key++}
                href={anchor.href}
                target="_blank"
                rel="noreferrer"
                className="inline-flex"
                title={alt}
              >
                <img
                  src={anchor.src}
                  alt={alt}
                  className="h-6 w-6 rounded-full border border-[var(--border-color)]/80"
                />
              </a>
            );
          })}
        </div>
      );
      i += 1;
      continue;
    }

    if (htmlTag.test(line)) {
      i += 1;
      continue;
    }

    const paragraph: string[] = [line.trim()];
    i += 1;

    while (i < lines.length) {
      const nextLine = lines[i].trimEnd();
      if (!nextLine.trim()) break;
      if (isBlockStart(nextLine)) break;
      if (htmlTag.test(nextLine)) break;
      paragraph.push(nextLine.trim());
      i += 1;
    }

    const text = paragraph.join(" ").trim();
    if (text) {
      nodes.push(
        <p key={key++} className="my-1.5 [overflow-wrap:anywhere]">
          {inlinePass(text)}
        </p>
      );
    }
  }

  return nodes;
}

/** As notas já desenhadas; recalcula só quando o texto muda. */
export function ReleaseNotesMarkdown({ source, className }: { source: string; className?: string }) {
  const nodes = useMemo(() => renderReleaseNotes(source), [source]);
  return <div className={className}>{nodes}</div>;
}
