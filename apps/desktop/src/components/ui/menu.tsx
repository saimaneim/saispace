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
            "bg-elevated shadow-popover absolute top-full z-40 mt-1 min-w-52 rounded-lg p-1",
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
                "focus-visible:ring-accent/40 flex h-9 w-full items-center gap-2.5 rounded-md px-3 text-left text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 disabled:opacity-40",
                danger
                  ? "text-coral-red hover:bg-coral-red/10"
                  : "text-mist hover:text-paper hover:bg-obsidian",
              )}
            >
              {Icon && <Icon className="h-4 w-4 flex-shrink-0" />}
              <span>{label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
