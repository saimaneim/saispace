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
          className="px-1.5 py-0.5 rounded bg-obsidian text-bone font-mono text-xs break-words"
        >
          {earliest.content}
        </code>,
      );
    } else if (earliest.type === "bold") {
      parts.push(
        <strong key={`b-${key++}`} className="font-semibold text-paper">
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
          className="text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent break-words inline text-left focus:outline-none focus-visible:ring-1 focus-visible:ring-accent/40 rounded-sm"
        >
          {earliest.content}
        </button>,
      );
    } else {
      parts.push(
        <span
          key={`l-${key++}`}
          title={earliest.extra}
          className="text-paper underline decoration-smoke underline-offset-2 break-words"
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
          "my-2 space-y-1 pl-4 text-sm text-mist",
          listBuffer.type === "ul" ? "list-disc" : "list-decimal",
        )}
      >
        {listBuffer.items.map((item, i) => (
          <li key={`li-${i}`} className="break-words leading-relaxed">
            {renderInline(item)}
          </li>
        ))}
      </Tag>,
    );
    listBuffer = null;
  };

  lines.forEach((line, idx) => {
    if (line.startsWith("```")) {
      flushList(`fl-${idx}`);
      if (inCodeBlock) {
        elements.push(
          <pre
            key={`${keyPrefix}-code-${idx}`}
            className="my-2.5 p-3.5 bg-obsidian rounded-md font-mono text-xs text-mist overflow-x-auto leading-relaxed"
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
      elements.push(<div key={`${keyPrefix}-hr-${idx}`} className="h-px bg-obsidian my-4" />);
      return;
    }

    if (line.startsWith("#### ")) {
      elements.push(
        <h4
          key={`${keyPrefix}-h4-${idx}`}
          className="text-xs font-semibold text-bone mt-3 mb-1 font-sans break-words"
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
          className="text-sm font-semibold text-paper mt-4 mb-1.5 font-sans break-words"
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
          className="text-base font-semibold text-paper tracking-tight mt-5 mb-2 font-sans break-words"
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
          className="text-lg font-bold text-paper tracking-tight mt-5 mb-2 font-sans break-words"
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
          className="bg-obsidian/60 px-3.5 py-2 rounded-md my-2 text-sm text-fog font-sans flex gap-2.5 min-w-0"
        >
          <span className="w-0.5 rounded-full bg-smoke flex-shrink-0" aria-hidden="true" />
          <span className="min-w-0 flex-1 italic break-words">{renderInline(line.slice(2))}</span>
        </blockquote>,
      );
      return;
    }

    if (!line.trim()) return;

    elements.push(
      <p
        key={`${keyPrefix}-p-${idx}`}
        className="text-sm text-mist leading-relaxed my-1.5 break-words"
      >
        {renderInline(line)}
      </p>,
    );
  });

  flushList("final");

  if (inCodeBlock && codeBuffer.length > 0) {
    elements.push(
      <pre
        key={`${keyPrefix}-code-final`}
        className="my-2.5 p-3.5 bg-obsidian rounded-md font-mono text-xs text-mist overflow-x-auto leading-relaxed"
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
          className="py-2 first:pt-0 last:pb-0 flex items-start gap-3 min-w-0"
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
        <div key={`section-${suffix}`} className="py-1.5 min-w-0">
          <p className="text-xs font-semibold text-paper mb-0.5 break-words">
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
        "max-w-[70ch] text-sm leading-relaxed text-mist select-text min-w-0",
        className,
      )}
    >
      {blocks}
    </div>
  );
}
