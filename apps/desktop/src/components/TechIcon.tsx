import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { cn } from "../lib/utils";

const ALIASES: Record<string, string> = {
  "react-dom": "react",
  "react-router-dom": "react router",
  "lucide-react": "lucide",
  "@types/node": "nodejs",
  node: "nodejs",
  "@types/react": "react",
  "@types/react-dom": "react",
  "@types/bun": "bun",
  "tauri-build": "tauri",
  "tailwind-merge": "tailwindcss",
  "tw-animate-css": "tailwindcss",
  "tailwindcss-animate": "tailwindcss",
  "prettier-plugin-tailwindcss": "tailwindcss",
  "drizzle-orm": "drizzle orm",
  "drizzle-kit": "drizzle orm",
  "better-auth": "better auth",
  pg: "postgresql",
  postgres: "postgresql",
  "@electric-sql/pglite": "postgresql",
  sqlx: "sqlite",
  rusqlite: "sqlite",
  tokio: "rust",
  serde: "rust",
  serde_json: "rust",
  rmcp: "mcp",
  "eslint-config-next": "eslint",
  "babel-plugin-react-compiler": "babel",
  baileys: "whatsapp",
  "@whiskeysockets/baileys": "whatsapp",
  "@google/genai": "gemini",
  "@googleapis/drive": "google drive",
  "@aws-sdk/client-s3": "aws",
  "class-variance-authority": "cva",
  claude: "claude ai",
  "claude code": "claude ai",
  opencode: "opencode",
  antigravity: "gemini",
};

/** Scopes whose packages all share the project's logo. */
const SCOPE_ICONS: Record<string, string> = {
  "@tauri-apps": "tauri",
  "@radix-ui": "radix ui",
  "@supabase": "supabase",
  "@tailwindcss": "tailwindcss",
  "@vitejs": "vite",
  "@biomejs": "biome",
  "@babel": "babel",
  "@hugeicons": "hugeicons",
  "@hono": "hono",
  "@tanstack": "tanstack",
  "@anthropic-ai": "anthropic",
  "@fontsource": "google fonts",
  "@fontsource-variable": "google fonts",
};

export function iconQuery(name: string): string {
  const lower = name.toLowerCase();
  if (lower in ALIASES) return ALIASES[lower];
  if (lower.startsWith("tauri-plugin-")) return "tauri";
  if (lower.startsWith("@types/")) {
    // @types/babel__traverse belongs to @babel/traverse.
    const base = lower.slice("@types/".length).replace(/^(.+)__(.+)$/, "@$1/$2");
    return base in ALIASES || base.startsWith("@") ? iconQuery(base) : "typescript";
  }
  const scope = lower.match(/^(@[^/]+)\//)?.[1];
  if (scope && scope in SCOPE_ICONS) return SCOPE_ICONS[scope];
  return lower.replace(/^@[^/]+\//, "");
}

/** npm name used as a last-resort icon source (its homepage favicon). */
function npmPackage(name: string): string | undefined {
  return /^(@[a-z0-9._-]+\/)?[a-z0-9._-]+$/i.test(name) && !name.startsWith("@types/")
    ? name
    : undefined;
}

const memo = new Map<string, Promise<string | null>>();

function loadIcon(query: string, pkg?: string): Promise<string | null> {
  let pending = memo.get(query);
  if (!pending) {
    pending = api.techIcon(query, pkg).catch(() => null);
    memo.set(query, pending);
  }
  return pending;
}

interface TechIconProps {
  name: string;
  className?: string;
}

export function TechIcon({ name, className }: TechIconProps) {
  const query = iconQuery(name);
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setSrc(null);
    loadIcon(query, npmPackage(name)).then((icon) => {
      if (alive && icon) setSrc(icon);
    });
    return () => {
      alive = false;
    };
  }, [query, name]);

  if (!src) {
    return (
      <span
        className={cn(
          "w-6 h-6 rounded-md bg-elevated text-fog text-xs font-semibold flex items-center justify-center flex-shrink-0",
          className,
        )}
      >
        {name.replace(/^@/, "").charAt(0).toUpperCase()}
      </span>
    );
  }
  return (
    <img
      src={src}
      alt=""
      width={24}
      height={24}
      className={cn("w-6 h-6 p-0.5 flex-shrink-0 object-contain", className)}
    />
  );
}
