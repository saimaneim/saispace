import type { ReactNode } from "react";
import { cn } from "../lib/utils";

export interface MarkdownProps {
  content: string;
  className?: string;
}

function renderInline(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  let remaining = text;
  let key = 0;

  while (remaining.length > 0) {
    const codeMatch = remaining.match(/`([^`]+)`/);
    const boldMatch = remaining.match(/\*\*([^*]+)\*\*/);

    let earliestMatch: {
      type: "code" | "bold";
      index: number;
      length: number;
      content: string;
    } | null = null;

    if (codeMatch && codeMatch.index !== undefined) {
      earliestMatch = {
        type: "code",
        index: codeMatch.index,
        length: codeMatch[0].length,
        content: codeMatch[1],
      };
    }

    if (boldMatch && boldMatch.index !== undefined) {
      if (!earliestMatch || boldMatch.index < earliestMatch.index) {
        earliestMatch = {
          type: "bold",
          index: boldMatch.index,
          length: boldMatch[0].length,
          content: boldMatch[1],
        };
      }
    }

    if (!earliestMatch) {
      parts.push(remaining);
      break;
    }

    if (earliestMatch.index > 0) {
      parts.push(remaining.slice(0, earliestMatch.index));
    }

    if (earliestMatch.type === "code") {
      parts.push(
        <code
          key={`c-${key++}`}
          className="px-1.5 py-0.5 rounded bg-obsidian text-bone font-mono text-xs"
        >
          {earliestMatch.content}
        </code>,
      );
    } else {
      parts.push(
        <strong key={`b-${key++}`} className="font-semibold text-paper">
          {earliestMatch.content}
        </strong>,
      );
    }

    remaining = remaining.slice(earliestMatch.index + earliestMatch.length);
  }

  return parts;
}

export function Markdown({ content, className }: MarkdownProps) {
  if (!content?.trim()) return null;

  const lines = content.split("\n");
  const elements: ReactNode[] = [];
  let inCodeBlock = false;
  let codeBuffer: string[] = [];
  let listBuffer: { type: "ul" | "ol"; items: string[] } | null = null;

  const flushList = (keyPrefix: string) => {
    if (!listBuffer) return;
    if (listBuffer.type === "ul") {
      elements.push(
        <ul key={`${keyPrefix}-ul`} className="my-2 space-y-1 pl-4 list-disc text-sm text-mist">
          {listBuffer.items.map((item, i) => (
            <li key={`li-${i}`}>{renderInline(item)}</li>
          ))}
        </ul>,
      );
    } else {
      elements.push(
        <ol key={`${keyPrefix}-ol`} className="my-2 space-y-1 pl-4 list-decimal text-sm text-mist">
          {listBuffer.items.map((item, i) => (
            <li key={`ol-${i}`}>{renderInline(item)}</li>
          ))}
        </ol>,
      );
    }
    listBuffer = null;
  };

  lines.forEach((line, idx) => {
    if (line.startsWith("```")) {
      flushList(`fl-${idx}`);
      if (inCodeBlock) {
        elements.push(
          <pre
            key={`code-${idx}`}
            className="my-3 p-3.5 bg-obsidian rounded-md font-mono text-xs text-mist overflow-x-auto leading-relaxed"
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
      if (listBuffer && listBuffer.type !== "ul") {
        flushList(`fl-ul-${idx}`);
      }
      if (!listBuffer) {
        listBuffer = { type: "ul", items: [] };
      }
      listBuffer.items.push(ulMatch[1]);
      return;
    }

    const olMatch = line.match(/^\d+\.\s+(.*)$/);
    if (olMatch) {
      if (listBuffer && listBuffer.type !== "ol") {
        flushList(`fl-ol-${idx}`);
      }
      if (!listBuffer) {
        listBuffer = { type: "ol", items: [] };
      }
      listBuffer.items.push(olMatch[1]);
      return;
    }

    flushList(`fl-post-${idx}`);

    if (line.startsWith("### ")) {
      elements.push(
        <h3 key={`h3-${idx}`} className="text-sm font-semibold text-paper mt-4 mb-1.5 font-sans">
          {renderInline(line.replace("### ", ""))}
        </h3>,
      );
      return;
    }

    if (line.startsWith("## ")) {
      elements.push(
        <h2
          key={`h2-${idx}`}
          className="text-base font-semibold text-paper tracking-tight mt-5 mb-2 font-sans"
        >
          {renderInline(line.replace("## ", ""))}
        </h2>,
      );
      return;
    }

    if (line.startsWith("# ")) {
      elements.push(
        <h1
          key={`h1-${idx}`}
          className="text-lg font-bold text-paper tracking-tight mt-6 mb-2 font-sans"
        >
          {renderInline(line.replace("# ", ""))}
        </h1>,
      );
      return;
    }

    if (line.startsWith("> ")) {
      elements.push(
        <blockquote
          key={`quote-${idx}`}
          className="bg-obsidian/60 px-3 py-2 rounded-md text-fog italic my-2 text-sm font-sans"
        >
          {renderInline(line.replace("> ", ""))}
        </blockquote>,
      );
      return;
    }

    const sectionMatch = line.match(/^\*\*(.+?)(?:\*\*:\s*|:\*\*\s*)(.*)$/);
    if (sectionMatch) {
      const keyLabel = sectionMatch[1];
      const valText = sectionMatch[2];
      elements.push(
        <div key={`sec-${idx}`} className="my-3">
          <span className="block text-xs uppercase tracking-wider text-fog font-medium mb-1">
            {keyLabel}
          </span>
          {valText ? (
            <div className="text-sm text-mist leading-relaxed">{renderInline(valText)}</div>
          ) : null}
        </div>,
      );
      return;
    }

    if (!line.trim()) {
      return;
    }

    elements.push(
      <p key={`p-${idx}`} className="text-sm text-mist leading-relaxed my-2">
        {renderInline(line)}
      </p>,
    );
  });

  flushList("final");

  if (inCodeBlock && codeBuffer.length > 0) {
    elements.push(
      <pre
        key="code-final"
        className="my-3 p-3.5 bg-obsidian rounded-md font-mono text-xs text-mist overflow-x-auto leading-relaxed"
      >
        <code>{codeBuffer.join("\n")}</code>
      </pre>,
    );
  }

  return (
    <div className={cn("max-w-[70ch] text-sm leading-relaxed text-mist select-text", className)}>
      {elements}
    </div>
  );
}
