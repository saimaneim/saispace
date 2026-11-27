import { ArrowLeft, Eye, EyeOff, Folder, FolderPlus, Loader2, Search } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, formatError } from "../lib/api";
import type { DirectoryListing } from "../lib/types";
import { cn } from "../lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./ui/dialog";

interface DirectoryPickerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (folderPath: string) => Promise<void>;
}

export function DirectoryPickerModal({ isOpen, onClose, onSelect }: DirectoryPickerModalProps) {
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [selectedIndex, setSelectedIndex] = useState<number>(0);
  const [loading, setLoading] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [search, setSearch] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  const fetchDirectory = useCallback(
    async (path?: string) => {
      setLoading(true);
      setErrorMessage(null);
      try {
        const res = await api.listDirectory(path, showHidden);
        setListing(res);
        setSelectedIndex(0);
        setSearch("");
      } catch (err) {
        setErrorMessage(formatError(err));
      } finally {
        setLoading(false);
      }
    },
    [showHidden],
  );

  useEffect(() => {
    if (isOpen) {
      fetchDirectory();
    }
  }, [isOpen, fetchDirectory]);

  const filteredEntries = (listing?.entries ?? []).filter((e) =>
    e.name.toLowerCase().includes(search.toLowerCase()),
  );

  const selectedEntry = filteredEntries[selectedIndex] ?? null;

  const handleEnterDirectory = (path: string) => {
    fetchDirectory(path);
  };

  const handleGoBack = () => {
    if (listing?.parent_path) {
      fetchDirectory(listing.parent_path);
    }
  };

  const handleConfirm = async (explicitPath?: string) => {
    if (!listing || isSubmitting) return;
    const targetPath = explicitPath || selectedEntry?.path || listing.current_path;

    setIsSubmitting(true);
    setErrorMessage(null);
    try {
      await onSelect(targetPath);
      onClose();
    } catch (err) {
      setErrorMessage(formatError(err));
    } finally {
      setIsSubmitting(false);
    }
  };

  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        e.preventDefault();
        handleConfirm();
        return;
      }

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev < filteredEntries.length - 1 ? prev + 1 : prev));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev > 0 ? prev - 1 : 0));
      } else if (e.key === "Enter" && document.activeElement !== searchInputRef.current) {
        e.preventDefault();
        if (selectedEntry) {
          handleEnterDirectory(selectedEntry.path);
        } else {
          handleConfirm();
        }
      } else if (
        e.key === "Backspace" &&
        search === "" &&
        document.activeElement !== searchInputRef.current
      ) {
        e.preventDefault();
        handleGoBack();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, filteredEntries, selectedIndex, search, selectedEntry, listing]);

  useEffect(() => {
    if (listRef.current) {
      const activeEl = listRef.current.children[selectedIndex] as HTMLElement;
      if (activeEl) {
        activeEl.scrollIntoView({ block: "nearest" });
      }
    }
  }, [selectedIndex]);

  const targetLabel = selectedEntry
    ? selectedEntry.name
    : (listing?.display_path ?? "carpeta actual");

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-xl p-0 gap-0 overflow-hidden bg-carbon select-none">
        <DialogTitle className="sr-only">Seleccionar carpeta</DialogTitle>
        <DialogDescription className="sr-only">
          Explora carpetas locales para añadir un proyecto a saispace.
        </DialogDescription>

        <div className="flex items-center justify-between p-4 bg-obsidian">
          <div className="flex items-center gap-2.5 min-w-0 flex-1">
            <button
              type="button"
              onClick={handleGoBack}
              disabled={!listing?.parent_path}
              aria-label="Volver al directorio anterior"
              className="p-1.5 rounded-md text-fog hover:text-paper hover:bg-elevated disabled:opacity-20 focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-colors duration-[120ms]"
            >
              <ArrowLeft className="w-4 h-4" />
            </button>

            <span
              className="font-mono text-xs font-medium text-paper truncate"
              title={listing?.current_path}
            >
              {listing?.display_path ?? "~/"}
            </span>
          </div>

          <div className="flex items-center gap-2 flex-shrink-0 mr-8">
            <button
              type="button"
              onClick={() => setShowHidden((prev) => !prev)}
              aria-label={showHidden ? "Ocultar carpetas ocultas" : "Mostrar carpetas ocultas"}
              className={cn(
                "flex items-center gap-1.5 px-2.5 h-8 rounded-md text-xs font-sans transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                showHidden
                  ? "bg-pulse-green/15 text-pulse-green"
                  : "bg-elevated text-fog hover:text-paper",
              )}
            >
              {showHidden ? <Eye className="w-3.5 h-3.5" /> : <EyeOff className="w-3.5 h-3.5" />}
              <span>Ocultos</span>
            </button>
          </div>
        </div>

        {errorMessage && (
          <div
            className="mx-4 mt-3 p-3 rounded-md bg-coral-red/10 text-coral-red text-xs font-sans"
            role="alert"
            aria-live="polite"
          >
            {errorMessage}
          </div>
        )}

        <div className="p-3 bg-obsidian/40">
          <div className="relative">
            <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
            <input
              ref={searchInputRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Filtrar carpetas…"
              aria-label="Filtrar carpetas"
              className="w-full h-9 pl-9 pr-3 text-sm bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-colors duration-[120ms]"
            />
          </div>
        </div>

        <div
          ref={listRef}
          role="listbox"
          aria-label="Lista de carpetas"
          className="flex-1 max-h-80 min-h-56 overflow-y-auto overscroll-contain p-2 space-y-0.5"
        >
          {loading ? (
            <div className="flex items-center justify-center py-16 text-fog gap-2">
              <Loader2 className="w-4 h-4 animate-spin motion-reduce:animate-none text-pulse-green" />
              <span className="text-sm font-sans text-fog">Leyendo carpetas…</span>
            </div>
          ) : filteredEntries.length > 0 ? (
            filteredEntries.map((entry, idx) => {
              const isSelected = idx === selectedIndex;
              return (
                <div
                  key={entry.path}
                  role="option"
                  aria-selected={isSelected}
                  onClick={() => setSelectedIndex(idx)}
                  onDoubleClick={() => handleEnterDirectory(entry.path)}
                  className={cn(
                    "group relative flex items-center justify-between pl-3.5 pr-3 h-10 rounded-md cursor-pointer text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                    isSelected
                      ? "bg-elevated text-paper font-medium"
                      : "text-mist hover:bg-obsidian hover:text-paper",
                  )}
                >
                  {isSelected && (
                    <span
                      className="absolute left-1 top-2.5 bottom-2.5 w-0.5 rounded-full bg-acid-lime"
                      aria-hidden="true"
                    />
                  )}
                  <div className="flex items-center gap-2.5 min-w-0 flex-1">
                    <Folder className="w-4 h-4 text-fog group-hover:text-mist flex-shrink-0" />
                    <span className="truncate tracking-tight font-sans text-sm">{entry.name}</span>
                  </div>

                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleConfirm(entry.path);
                    }}
                    aria-label={`Añadir ${entry.name}`}
                    className="opacity-0 group-hover:opacity-100 px-2.5 h-7 rounded-md text-xs font-medium bg-obsidian hover:bg-acid-lime hover:text-void text-pulse-green transition-[opacity,background-color,color] duration-[120ms] focus:opacity-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                  >
                    + Añadir
                  </button>
                </div>
              );
            })
          ) : (
            <div className="py-16 text-center text-sm text-fog font-sans">
              Sin carpetas en esta ubicación
            </div>
          )}
        </div>

        <div className="p-3 bg-obsidian flex items-center justify-between text-xs text-fog">
          <span className="text-xs text-ash font-sans">↑↓ navegar · Enter entrar · Esc cerrar</span>

          <div className="flex items-center gap-2">
            {selectedEntry && (
              <button
                type="button"
                onClick={() => handleConfirm(listing?.current_path)}
                disabled={isSubmitting || !listing}
                className="text-xs text-fog hover:text-paper transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 rounded-md px-2.5 py-1.5"
                title="Añadir la carpeta abierta actualmente"
              >
                Añadir actual
              </button>
            )}

            <button
              type="button"
              onClick={() => handleConfirm()}
              disabled={isSubmitting || !listing}
              className="h-9 px-4 rounded-md bg-acid-lime text-void hover:brightness-105 text-sm font-medium transition-[filter] duration-[120ms] disabled:opacity-50 focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 flex items-center gap-2"
            >
              <FolderPlus className="w-4 h-4 text-void" />
              <span className="truncate max-w-48">Añadir &ldquo;{targetLabel}&rdquo;</span>
            </button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
