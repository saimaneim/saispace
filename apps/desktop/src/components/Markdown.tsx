import { Info, Lightbulb, MapPin, MessageCircleQuestion } from "lucide-react";
import type { ReactNode } from "react";
import { api } from "../lib/api";
import { cn } from "../lib/utils";

export interface MarkdownProps {
  content: string;
  className?: string;
}

const STRUCTURED_BOLD_RE = /^\*\*(.+?)(?:\*\*:\s*|:\*\*\s*)(.*)$/;
const STRUCTURED_PLAIN_RE = /^(What|Why|Where|Learned):\s*(.*)$/i;

const ROLE_META: Record<string, { icon: typeof Info; title: string }> = {
  what: { icon: Info, title: "Qué (What)" },
  why: { icon: MessageCircleQuestion, title: "Por qué (Why)" },
  where: { icon: MapPin, title: "Dónde (Where)" },
  learned: { icon: Lightbulb, title: "Aprendido (Learned)" },
};

function renderInline(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  let remaining = text;
  let key = 0;

  while (remaining.length > 0) {
    const codeMatch = remaining.match(/`([^`]+)`/);
    const boldMatch = remaining.match(/\*\*([^*]+)\*\*/);
    const linkMatch = remaining.match(/\[([^\]]+)\]\(([^)\s]+)\)/);

    let earliest: {
      type: "code" | "bold" | "link";
      index: number;
      length: number;
      content: string;
      extra?: string;
    } | null = null;

    if (codeMatch && codeMatch.index !== undefined) {
      earliest = {
        type: "code",
        index: codeMatch.index,
        length: codeMatch[0].length,
        content: codeMatch[1],
      };
    }

    if (boldMatch && boldMatch.index !== undefined) {
      if (!earliest || boldMatch.index < earliest.index) {
        earliest = {
          type: "bold",
          index: boldMatch.index,
          length: boldMatch[0].length,
          content: boldMatch[1],
        };
      }
    }

    if (linkMatch && linkMatch.index !== undefined) {
      if (!earliest || linkMatch.index < earliest.index) {
        earliest = {
          type: "link",
          index: linkMatch.index,
          length: linkMatch[0].length,
          content: linkMatch[1],
          extra: linkMatch[2],
        };
      }
    }

    if (!earliest) {
      parts.push(remaining);
      break;
    }

    if (earliest.index > 0) {
      parts.push(remaining.slice(0, earliest.index));
    }

    if (earliest.type === "code") {
      parts.push(
        <code
          key={`c-${key++}`}
          className="bg-obsidian text-bone rounded px-1.5 py-0.5 font-mono text-xs break-words"
        >
          {earliest.content}
        </code>,
      );
    } else if (earliest.type === "bold") {
      parts.push(
        <strong key={`b-${key++}`} className="text-paper font-semibold">
          {earliest.content}
        </strong>,
      );
    } else if (earliest.extra && /^https?:\/\//i.test(earliest.extra)) {
      const href = earliest.extra;
      parts.push(
        <button
          key={`l-${key++}`}
          type="button"
          title={href}
          onClick={() => void api.openUrl(href)}
          className="text-accent decoration-accent/40 hover:decoration-accent focus-visible:ring-accent/40 inline rounded-sm text-left break-words underline underline-offset-2 focus:outline-none focus-visible:ring-1"
        >
          {earliest.content}
        </button>,
      );
    } else {
      parts.push(
        <span
          key={`l-${key++}`}
          title={earliest.extra}
          className="text-paper decoration-smoke break-words underline underline-offset-2"
        >
          {earliest.content}
        </span>,
      );
    }

    remaining = remaining.slice(earliest.index + earliest.length);
  }

  return parts;
}

function matchStructuredHeader(line: string): { label: string; value: string } | null {
  const boldMatch = line.match(STRUCTURED_BOLD_RE);
  if (boldMatch) {
    return { label: boldMatch[1].trim(), value: boldMatch[2] };
  }
  const plainMatch = line.match(STRUCTURED_PLAIN_RE);
  if (plainMatch) {
    const raw = plainMatch[1];
    const normalized = raw.charAt(0).toUpperCase() + raw.slice(1).toLowerCase();
    return { label: normalized, value: plainMatch[2] };
  }
  return null;
}

/** `| a | b |` -> ["a", "b"]; `\|` stays a literal pipe. */
function splitRow(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split(/(?<!\\)\|/)
    .map((cell) => cell.trim().replace(/\\\|/g, "|"));
}

function renderFlowBlocks(lines: string[], keyPrefix: string): ReactNode[] {
  const elements: ReactNode[] = [];
  let inCodeBlock = false;
  let codeBuffer: string[] = [];
  let listBuffer: { type: "ul" | "ol"; items: string[] } | null = null;

  const flushList = (suffix: string) => {
    if (!listBuffer) return;
    const Tag = listBuffer.type;
    elements.push(
      <Tag
        key={`${keyPrefix}-${suffix}`}
        className={cn(
          "text-mist my-2 space-y-1 pl-4 text-sm",
          listBuffer.type === "ul" ? "list-disc" : "list-decimal",
        )}
      >
        {listBuffer.items.map((item, i) => (
          <li key={`li-${i}`} className="leading-relaxed break-words">
            {renderInline(item)}
          </li>
        ))}
      </Tag>,
    );
    listBuffer = null;
  };

  let tableBuffer: string[] = [];
  const flushTable = (suffix: string) => {
    if (tableBuffer.length === 0) return;
    const rows = tableBuffer.map(splitRow);
    tableBuffer = [];
    const hasHeader = rows.length > 1 && rows[1].every((c) => /^:?-{2,}:?$/.test(c));
    const head = hasHeader ? rows[0] : null;
    const body = hasHeader ? rows.slice(2) : rows;
    elements.push(
      <div key={`${keyPrefix}-tbl-${suffix}`} className="my-3 overflow-x-auto">
        <table className="w-full border-collapse text-xs">
          {head && (
            <thead>
              <tr className="border-obsidian border-b">
                {head.map((cell, i) => (
                  <th key={`th-${i}`} className="text-paper px-2.5 py-1.5 text-left font-semibold">
                    {renderInline(cell)}
                  </th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {body.map((row, r) => (
              <tr key={`tr-${r}`} className="border-obsidian/60 border-b last:border-0">
                {row.map((cell, i) => (
                  <td key={`td-${i}`} className="text-mist px-2.5 py-1.5 align-top break-words">
                    {renderInline(cell)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>,
    );
  };

  lines.forEach((line, idx) => {
    if (!inCodeBlock && /^\s*\|.*\|\s*$/.test(line)) {
      flushList(`fl-t-${idx}`);
      tableBuffer.push(line);
      return;
    }
    flushTable(`${idx}`);

    if (line.startsWith("```")) {
      flushList(`fl-${idx}`);
      if (inCodeBlock) {
        elements.push(
          <pre
            key={`${keyPrefix}-code-${idx}`}
            className="bg-obsidian text-mist my-2.5 overflow-x-auto rounded-md p-3.5 font-mono text-xs leading-relaxed"
          >
            <code>{codeBuffer.join("\n")}</code>
          </pre>,
        );
        codeBuffer = [];
        inCodeBlock = false;
      } else {
        inCodeBlock = true;
        codeBuffer = [];
      }
      return;
    }

    if (inCodeBlock) {
      codeBuffer.push(line);
      return;
    }

    const ulMatch = line.match(/^[-*]\s+(.*)$/);
    if (ulMatch) {
      if (listBuffer && listBuffer.type !== "ul") flushList(`fl-ul-${idx}`);
      if (!listBuffer) listBuffer = { type: "ul", items: [] };
      listBuffer.items.push(ulMatch[1]);
      return;
    }

    const olMatch = line.match(/^\d+\.\s+(.*)$/);
    if (olMatch) {
      if (listBuffer && listBuffer.type !== "ol") flushList(`fl-ol-${idx}`);
      if (!listBuffer) listBuffer = { type: "ol", items: [] };
      listBuffer.items.push(olMatch[1]);
      return;
    }

    flushList(`fl-post-${idx}`);

    if (/^[-*_]{3,}\s*$/.test(line)) {
      elements.push(<div key={`${keyPrefix}-hr-${idx}`} className="bg-obsidian my-4 h-px" />);
      return;
    }

    if (line.startsWith("#### ")) {
      elements.push(
        <h4
          key={`${keyPrefix}-h4-${idx}`}
          className="text-bone mt-3 mb-1 font-sans text-xs font-semibold break-words"
        >
          {renderInline(line.slice(5))}
        </h4>,
      );
      return;
    }

    if (line.startsWith("### ")) {
      elements.push(
        <h3
          key={`${keyPrefix}-h3-${idx}`}
          className="text-paper mt-4 mb-1.5 font-sans text-sm font-semibold break-words"
        >
          {renderInline(line.slice(4))}
        </h3>,
      );
      return;
    }

    if (line.startsWith("## ")) {
      elements.push(
        <h2
          key={`${keyPrefix}-h2-${idx}`}
          className="text-paper mt-5 mb-2 font-sans text-base font-semibold tracking-tight break-words"
        >
          {renderInline(line.slice(3))}
        </h2>,
      );
      return;
    }

    if (line.startsWith("# ")) {
      elements.push(
        <h1
          key={`${keyPrefix}-h1-${idx}`}
          className="text-paper mt-5 mb-2 font-sans text-lg font-bold tracking-tight break-words"
        >
          {renderInline(line.slice(2))}
        </h1>,
      );
      return;
    }

    if (line.startsWith("> ")) {
      elements.push(
        <blockquote
          key={`${keyPrefix}-quote-${idx}`}
          className="bg-obsidian/60 text-fog my-2 flex min-w-0 gap-2.5 rounded-md px-3.5 py-2 font-sans text-sm"
        >
          <span className="bg-smoke w-0.5 flex-shrink-0 rounded-full" aria-hidden="true" />
          <span className="min-w-0 flex-1 break-words italic">{renderInline(line.slice(2))}</span>
        </blockquote>,
      );
      return;
    }

    if (!line.trim()) return;

    elements.push(
      <p
        key={`${keyPrefix}-p-${idx}`}
        className="text-mist my-1.5 text-sm leading-relaxed break-words"
      >
        {renderInline(line)}
      </p>,
    );
  });

  flushList("final");
  flushTable("final");

  if (inCodeBlock && codeBuffer.length > 0) {
    elements.push(
      <pre
        key={`${keyPrefix}-code-final`}
        className="bg-obsidian text-mist my-2.5 overflow-x-auto rounded-md p-3.5 font-mono text-xs leading-relaxed"
      >
        <code>{codeBuffer.join("\n")}</code>
      </pre>,
    );
  }

  return elements;
}

export function Markdown({ content, className }: MarkdownProps) {
  if (!content?.trim()) return null;

  const lines = content.split("\n");
  const blocks: ReactNode[] = [];
  let plainBuffer: string[] = [];
  let activeSection: { label: string; lines: string[] } | null = null;
  let inCodeBlock = false;

  const flushPlain = (suffix: string) => {
    if (plainBuffer.length === 0) return;
    blocks.push(...renderFlowBlocks(plainBuffer, `plain-${suffix}`));
    plainBuffer = [];
  };

  const flushSection = (suffix: string) => {
    if (!activeSection) return;
    const roleKey = activeSection.label.toLowerCase();
    const role = Object.hasOwn(ROLE_META, roleKey) ? ROLE_META[roleKey] : undefined;
    const body = renderFlowBlocks(activeSection.lines, `sec-${suffix}`);

    if (role) {
      const Icon = role.icon;
      blocks.push(
        <div
          key={`section-${suffix}`}
          className="flex min-w-0 items-start gap-3 py-2 first:pt-0 last:pb-0"
        >
          <span title={role.title} className="mt-1 flex-shrink-0">
            <Icon
              className={cn("h-4 w-4", roleKey === "learned" ? "text-accent" : "text-fog")}
              aria-label={activeSection.label}
            />
          </span>
          <div className="min-w-0 flex-1 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">{body}</div>
        </div>,
      );
    } else {
      blocks.push(
        <div key={`section-${suffix}`} className="min-w-0 py-1.5">
          <p className="text-paper mb-0.5 text-xs font-semibold break-words">
            {activeSection.label}
          </p>
          <div className="min-w-0 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">{body}</div>
        </div>,
      );
    }
    activeSection = null;
  };

  lines.forEach((line, idx) => {
    if (line.startsWith("```")) {
      inCodeBlock = !inCodeBlock;
      if (activeSection) activeSection.lines.push(line);
      else plainBuffer.push(line);
      return;
    }

    if (!inCodeBlock) {
      const section = matchStructuredHeader(line);
      if (section) {
        flushPlain(`pre-${idx}`);
        flushSection(`pre-${idx}`);
        activeSection = {
          label: section.label,
          lines: section.value ? [section.value] : [],
        };
        return;
      }

      if (activeSection && (/^#{1,4}\s/.test(line) || /^[-*_]{3,}\s*$/.test(line))) {
        flushSection(`head-${idx}`);
      }
    }

    if (activeSection) {
      activeSection.lines.push(line);
    } else {
      plainBuffer.push(line);
    }
  });

  flushPlain("end");
  flushSection("end");

  return (
    <div
      className={cn(
        "text-mist max-w-[70ch] min-w-0 text-sm leading-relaxed select-text",
        className,
      )}
    >
      {blocks}
    </div>
  );
}
