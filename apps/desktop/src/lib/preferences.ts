export type DefaultTabId = "context" | "plan" | "tech" | "memory";

const DEFAULT_TAB_KEY = "saispace.default-tab";

export function readDefaultTab(): DefaultTabId {
  try {
    const saved = localStorage.getItem(DEFAULT_TAB_KEY);
    if (saved === "context" || saved === "plan" || saved === "tech" || saved === "memory") {
      return saved;
    }
  } catch {
    return "plan";
  }
  return "plan";
}

export function saveDefaultTab(tab: DefaultTabId): void {
  try {
    localStorage.setItem(DEFAULT_TAB_KEY, tab);
  } catch {
    return;
  }
}
