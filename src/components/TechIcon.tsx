import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { cn } from "../lib/utils";

const ALIASES: Record<string, string> = {
  "react-dom": "react",
  "lucide-react": "lucide",
  "@biomejs/biome": "biome",
  "@vitejs/plugin-react": "vite",
  "@tailwindcss/vite": "tailwindcss",
  "@types/node": "nodejs",
  "@types/react": "react",
  "@types/react-dom": "react",
  "tauri-build": "tauri",
  "tauri-plugin-opener": "tauri",
  sqlx: "sqlite",
  rusqlite: "sqlite",
  tokio: "rust",
  serde: "rust",
  serde_json: "rust",
  rmcp: "mcp",
  claude: "claude",
  opencode: "opencode",
};

export function iconQuery(name: string): string {
  const lower = name.toLowerCase();
  if (lower in ALIASES) return ALIASES[lower];
  if (lower.startsWith("@tauri-apps/")) return "tauri";
  if (lower.startsWith("@radix-ui/")) return "radix";
  if (lower.startsWith("@types/")) return "typescript";
  if (lower.startsWith("@supabase/")) return "supabase";
  return lower.replace(/^@[^/]+\//, "");
}

const memo = new Map<string, Promise<string | null>>();

function loadIcon(query: string): Promise<string | null> {
  let pending = memo.get(query);
  if (!pending) {
    pending = api.techIcon(query).catch(() => null);
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
    loadIcon(query).then((svg) => {
      if (alive && svg) setSrc(`data:image/svg+xml;utf8,${encodeURIComponent(svg)}`);
    });
    return () => {
      alive = false;
    };
  }, [query]);

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
