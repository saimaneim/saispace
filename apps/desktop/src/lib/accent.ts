export const ACCENTS = [
  { id: "blue", label: "Azul", value: "#3b82f6" },
  { id: "indigo", label: "Índigo", value: "#6366f1" },
  { id: "violet", label: "Violeta", value: "#8b5cf6" },
  { id: "teal", label: "Turquesa", value: "#14b8a6" },
  { id: "green", label: "Verde", value: "#22c55e" },
  { id: "orange", label: "Naranja", value: "#f97316" },
  { id: "pink", label: "Rosa", value: "#ec4899" },
] as const;

const KEY = "saispace.accent";
export const DEFAULT_ACCENT = ACCENTS[0].value;

function srgbToLinear(channel: number): number {
  const c = channel / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function inkFor(hex: string): string {
  const n = Number.parseInt(hex.slice(1), 16);
  const r = srgbToLinear((n >> 16) & 255);
  const g = srgbToLinear((n >> 8) & 255);
  const b = srgbToLinear(n & 255);
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.3 ? "#141414" : "#ffffff";
}

export function readAccent(): string {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved && /^#[0-9a-f]{6}$/i.test(saved)) return saved.toLowerCase();
  } catch {
    return DEFAULT_ACCENT;
  }
  return DEFAULT_ACCENT;
}

export function applyAccent(hex: string): void {
  const root = document.documentElement.style;
  root.setProperty("--color-accent", hex);
  root.setProperty("--color-accent-ink", inkFor(hex));
}

export function saveAccent(hex: string): void {
  const normalized = hex.toLowerCase();
  applyAccent(normalized);
  try {
    localStorage.setItem(KEY, normalized);
  } catch {
    return;
  }
}
