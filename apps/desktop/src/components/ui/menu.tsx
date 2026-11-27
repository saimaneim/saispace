import type { LucideIcon } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { cn } from "../../lib/utils";

interface MenuItem {
  label: string;
  icon?: LucideIcon;
  danger?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}

interface MenuProps {
  trigger: (props: { open: boolean; toggle: () => void }) => ReactNode;
  items: MenuItem[];
  align?: "left" | "right";
}

export function Menu({ trigger, items, align = "right" }: MenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      {trigger({ open, toggle: () => setOpen((v) => !v) })}
      {open && (
        <div
          role="menu"
          className={cn(
            "absolute top-full mt-1 z-40 min-w-52 bg-elevated rounded-lg p-1 shadow-popover",
            align === "right" ? "right-0" : "left-0",
          )}
        >
          {items.map(({ label, icon: Icon, danger, disabled, onSelect }) => (
            <button
              key={label}
              type="button"
              role="menuitem"
              disabled={disabled}
              onClick={() => {
                setOpen(false);
                onSelect();
              }}
              className={cn(
                "w-full flex items-center gap-2.5 text-left px-3 h-9 text-sm rounded-md transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 disabled:opacity-40",
                danger
                  ? "text-coral-red hover:bg-coral-red/10"
                  : "text-mist hover:text-paper hover:bg-obsidian",
              )}
            >
              {Icon && <Icon className="w-4 h-4 flex-shrink-0" />}
              <span>{label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
