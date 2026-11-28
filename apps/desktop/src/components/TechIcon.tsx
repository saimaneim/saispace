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
  oxlint: "oxc",
  "@xyflow/react": "react",
  vue: "vue",
  "vue-router": "vue",
  "solid-js": "solid",
  "@sveltejs/kit": "svelte",
  "next-auth": "nextdotjs",
  "@prisma/client": "prisma",
  mongoose: "mongodb",
  ioredis: "redis",
  "socket.io": "socket.io",
  "socket.io-client": "socket.io",
  "@reduxjs/toolkit": "redux",
  "react-redux": "redux",
  three: "three.js",
  "@react-three/fiber": "three.js",
  "chart.js": "chart.js",
  "framer-motion": "framer",
  "styled-components": "styled components",
  "@playwright/test": "playwright",
  xlsx: "microsoft excel",
  exceljs: "microsoft excel",
  "react-hook-form": "react hook form",
  "@trpc/server": "trpc",
  "@trpc/client": "trpc",
  "@sentry/node": "sentry",
  "@sentry/react": "sentry",
  "@vercel/analytics": "vercel",
  openai: "openai",
  "@langchain/core": "langchain",
  stripe: "stripe",
  firebase: "firebase",
  "firebase-admin": "firebase",
  "@nestjs/core": "nestjs",
  "@nestjs/common": "nestjs",
  "@angular/core": "angular",
  axum: "rust",
  "actix-web": "rust",
  reqwest: "rust",
  clap: "rust",
  anyhow: "rust",
  thiserror: "rust",
  tracing: "rust",
  "tracing-subscriber": "rust",
  rayon: "rust",
  chrono: "rust",
  regex: "rust",
  uuid: "rust",
  tempfile: "rust",
  flate2: "rust",
  getrandom: "rust",
  globset: "rust",
  schemars: "rust",
  base64: "rust",
  chacha20poly1305: "rust",
  serde_yaml: "rust",
  torch: "pytorch",
  "scikit-learn": "scikit-learn",
  sklearn: "scikit-learn",
  requests: "python",
  uvicorn: "fastapi",
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
  "@nestjs": "nestjs",
  "@angular": "angular",
  "@sveltejs": "svelte",
  "@prisma": "prisma",
  "@trpc": "trpc",
  "@sentry": "sentry",
  "@vercel": "vercel",
  "@storybook": "storybook",
  "@testing-library": "testing library",
  "@reduxjs": "redux",
  "@react-three": "three.js",
  "@langchain": "langchain",
  "@aws-sdk": "aws",
  "@google-cloud": "google cloud",
  "@firebase": "firebase",
  "@stripe": "stripe",
  "@playwright": "playwright",
  "@xyflow": "react",
};

export function iconQuery(name: string): string {
  const lower = name.toLowerCase();
  if (lower in ALIASES) return ALIASES[lower];
  if (lower.startsWith("tauri-plugin-")) return "tauri";
  if (lower.startsWith("@types/")) {
    // @types/babel__traverse belongs to @babel/traverse; typings share the
    // logo of the library they describe.
    const base = lower.slice("@types/".length).replace(/^(.+)__(.+)$/, "@$1/$2");
    return iconQuery(base);
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

const tones = new Map<string, Promise<boolean>>();

/** Whether a logo is too dark to read on the dark UI (average luminance of
 * its visible pixels). Such logos get a light tile behind them. */
function isDarkLogo(src: string): Promise<boolean> {
  let pending = tones.get(src);
  if (!pending) {
    pending = new Promise<boolean>((resolve) => {
      const img = new Image();
      img.onload = () => {
        try {
          const size = 24;
          const canvas = document.createElement("canvas");
          canvas.width = size;
          canvas.height = size;
          const ctx = canvas.getContext("2d", { willReadFrequently: true });
          if (!ctx) return resolve(false);
          ctx.drawImage(img, 0, 0, size, size);
          const { data } = ctx.getImageData(0, 0, size, size);
          let sum = 0;
          let count = 0;
          for (let i = 0; i < data.length; i += 4) {
            if (data[i + 3] < 128) continue;
            sum += 0.2126 * data[i] + 0.7152 * data[i + 1] + 0.0722 * data[i + 2];
            count++;
          }
          resolve(count > 0 && sum / count < 70);
        } catch {
          resolve(false);
        }
      };
      img.onerror = () => resolve(false);
      img.src = src;
    });
    tones.set(src, pending);
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
  const [dark, setDark] = useState(false);

  useEffect(() => {
    let alive = true;
    setSrc(null);
    setDark(false);
    loadIcon(query, npmPackage(name)).then(async (icon) => {
      if (!alive || !icon) return;
      const tooDark = await isDarkLogo(icon);
      if (!alive) return;
      setDark(tooDark);
      setSrc(icon);
    });
    return () => {
      alive = false;
    };
  }, [query, name]);

  if (!src) {
    return (
      <span
        className={cn(
          "bg-elevated text-fog flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md text-xs font-semibold",
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
      className={cn(
        "h-6 w-6 flex-shrink-0 object-contain p-0.5",
        dark && "bg-paper rounded-[5px] p-[3px]",
        className,
      )}
    />
  );
}
