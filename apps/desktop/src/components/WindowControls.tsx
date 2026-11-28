import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

export function WindowControls() {
  const [isMaximized, setIsMaximized] = useState(false);
  const appWindow = useMemo(() => getCurrentWindow(), []);

  useEffect(() => {
    let mounted = true;

    const syncMaximizedState = async () => {
      try {
        const maximized = await appWindow.isMaximized();
        if (mounted) {
          setIsMaximized(maximized);
        }
      } catch {
        return;
      }
    };

    syncMaximizedState();

    const unlistenResize = appWindow.onResized?.(() => {
      syncMaximizedState();
    });

    return () => {
      mounted = false;
      unlistenResize?.then((dispose) => dispose());
    };
  }, [appWindow]);

  const handleMinimize = async () => {
    try {
      await appWindow.minimize();
    } catch {
      return;
    }
  };

  const handleToggleMaximize = async () => {
    try {
      await appWindow.toggleMaximize();
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    } catch {
      return;
    }
  };

  const handleClose = async () => {
    try {
      await appWindow.close();
    } catch {
      return;
    }
  };

  const base =
    "h-full w-11 flex items-center justify-center text-fog transition-colors duration-[120ms] focus:outline-none";

  return (
    <div className="pointer-events-auto flex h-full flex-shrink-0 items-center">
      <button
        type="button"
        onClick={handleMinimize}
        aria-label="Minimizar ventana"
        className={`${base} hover:text-paper hover:bg-elevated`}
      >
        <Minus className="h-4 w-4" />
      </button>
      <button
        type="button"
        onClick={handleToggleMaximize}
        aria-label={isMaximized ? "Restaurar ventana" : "Maximizar ventana"}
        className={`${base} hover:text-paper hover:bg-elevated`}
      >
        {isMaximized ? (
          <Copy className="h-3.5 w-3.5 rotate-180" />
        ) : (
          <Square className="h-3.5 w-3.5" />
        )}
      </button>
      <button
        type="button"
        onClick={handleClose}
        aria-label="Cerrar ventana"
        className={`${base} hover:text-paper hover:bg-coral-red`}
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  );
}
