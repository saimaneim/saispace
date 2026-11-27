export type DefaultTabId = "context" | "plan" | "history" | "tech" | "memory";

const DEFAULT_TAB_KEY = "saispace.default-tab";
const TAB_IDS: readonly string[] = ["context", "plan", "history", "tech", "memory"];

const isTabId = (value: string | null): value is DefaultTabId =>
  value !== null && TAB_IDS.includes(value);

export function readDefaultTab(): DefaultTabId {
  try {
    const saved = localStorage.getItem(DEFAULT_TAB_KEY);
    return isTabId(saved) ? saved : "plan";
  } catch {
    return "plan";
  }
}

export function saveDefaultTab(tab: DefaultTabId): void {
  try {
    localStorage.setItem(DEFAULT_TAB_KEY, tab);
  } catch {
    return;
  }
}
