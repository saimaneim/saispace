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
  const currentPathRef = useRef<string | undefined>(undefined);
  const fetchSeq = useRef(0);

  const fetchDirectory = useCallback(async (path: string | undefined, hidden: boolean) => {
    const seq = ++fetchSeq.current;
    setLoading(true);
    setErrorMessage(null);
    try {
      const res = await api.listDirectory(path, hidden);
      if (seq !== fetchSeq.current) return;
      currentPathRef.current = res.current_path;
      setListing(res);
      setSelectedIndex(0);
      setSearch("");
    } catch (err) {
      if (seq === fetchSeq.current) setErrorMessage(formatError(err));
    } finally {
      if (seq === fetchSeq.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!isOpen) {
      currentPathRef.current = undefined;
      return;
    }
    void fetchDirectory(currentPathRef.current, showHidden);
  }, [isOpen, showHidden, fetchDirectory]);

  const filteredEntries = (listing?.entries ?? []).filter((e) =>
    e.name.toLowerCase().includes(search.toLowerCase()),
  );

  const selectedEntry = filteredEntries[selectedIndex] ?? null;

  const handleEnterDirectory = useCallback(
    (path: string) => {
      void fetchDirectory(path, showHidden);
    },
    [fetchDirectory, showHidden],
  );

  const handleGoBack = useCallback(() => {
    if (listing?.parent_path) {
      void fetchDirectory(listing.parent_path, showHidden);
    }
  }, [listing?.parent_path, fetchDirectory, showHidden]);

  const handleConfirm = useCallback(
    async (explicitPath?: string) => {
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
    },
    [listing, isSubmitting, selectedEntry?.path, onSelect, onClose],
  );

  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        e.preventDefault();
        void handleConfirm();
        return;
      }

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev < filteredEntries.length - 1 ? prev + 1 : prev));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev > 0 ? prev - 1 : 0));
      } else if (e.key === "Enter" && document.activeElement !== searchInputRef.current) {
        const target = e.target as HTMLElement | null;
        if (target?.closest("button, a")) return;
        e.preventDefault();
        if (selectedEntry) {
          handleEnterDirectory(selectedEntry.path);
        } else {
          void handleConfirm();
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
  }, [
    isOpen,
    filteredEntries.length,
    search,
    selectedEntry,
    handleConfirm,
    handleEnterDirectory,
    handleGoBack,
  ]);

  useEffect(() => {
    if (listRef.current) {
      const activeEl = listRef.current.children[selectedIndex] as HTMLElement | undefined;
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
      <DialogContent className="bg-carbon max-w-xl gap-0 overflow-hidden p-0 select-none">
        <DialogTitle className="sr-only">Seleccionar carpeta</DialogTitle>
        <DialogDescription className="sr-only">
          Explora carpetas locales para añadir un proyecto a saispace.
        </DialogDescription>

        <div className="bg-obsidian flex items-center justify-between px-4 py-3.5">
          <div className="flex min-w-0 flex-1 items-center gap-2.5">
            <button
              type="button"
              onClick={handleGoBack}
              disabled={!listing?.parent_path}
              aria-label="Volver al directorio anterior"
              title="Subir una carpeta"
              className="text-fog hover:text-paper hover:bg-elevated focus-visible:ring-accent/40 rounded-md p-1.5 transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 disabled:opacity-20"
            >
              <ArrowLeft className="h-4 w-4" />
            </button>

            <span
              className="text-paper truncate font-mono text-xs font-medium"
              title={listing?.current_path}
            >
              {listing?.display_path ?? "~/"}
            </span>
          </div>

          <div className="mr-9 flex flex-shrink-0 items-center gap-2">
            <button
              type="button"
              onClick={() => setShowHidden((prev) => !prev)}
              aria-label={showHidden ? "Ocultar carpetas ocultas" : "Mostrar carpetas ocultas"}
              title={showHidden ? "Ocultar ocultos" : "Mostrar ocultos"}
              className={cn(
                "focus-visible:ring-accent/40 flex h-7 items-center gap-1.5 rounded-md px-2.5 font-sans text-xs transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                showHidden ? "bg-accent/15 text-accent" : "bg-elevated text-fog hover:text-paper",
              )}
            >
              {showHidden ? <Eye className="h-3.5 w-3.5" /> : <EyeOff className="h-3.5 w-3.5" />}
              <span>Ocultos</span>
            </button>
          </div>
        </div>

        {errorMessage && (
          <div
            className="bg-coral-red/10 text-coral-red mx-4 mt-3 rounded-md p-3 font-sans text-xs"
            role="alert"
            aria-live="polite"
          >
            {errorMessage}
          </div>
        )}

        <div className="bg-obsidian/40 p-3">
          <div className="relative">
            <Search className="text-fog pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2" />
            <input
              ref={searchInputRef}
              type="text"
              value={search}
              onChange={(e) => {
                setSearch(e.target.value);
                setSelectedIndex(0);
              }}
              placeholder="Filtrar carpetas…"
              aria-label="Filtrar carpetas"
              className="bg-obsidian text-paper placeholder:text-fog focus-visible:ring-accent/40 h-9 w-full rounded-md pr-3 pl-9 text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
            />
          </div>
        </div>

        <div
          ref={listRef}
          role="listbox"
          aria-label="Lista de carpetas"
          className="max-h-80 min-h-56 flex-1 space-y-0.5 overflow-y-auto overscroll-contain p-2"
        >
          {loading ? (
            <div className="text-fog flex items-center justify-center gap-2 py-16">
              <Loader2 className="text-accent h-4 w-4 animate-spin motion-reduce:animate-none" />
              <span className="text-fog font-sans text-sm">Leyendo…</span>
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
                    "group focus-visible:ring-accent/40 relative flex h-10 cursor-pointer items-center justify-between rounded-md pr-3 pl-3.5 text-sm transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2",
                    isSelected
                      ? "bg-elevated text-paper font-medium"
                      : "text-mist hover:bg-obsidian hover:text-paper",
                  )}
                >
                  <div className="flex min-w-0 flex-1 items-center gap-2.5">
                    <Folder className="text-fog group-hover:text-mist h-4 w-4 flex-shrink-0" />
                    <span className="truncate font-sans text-sm tracking-tight">{entry.name}</span>
                  </div>

                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleConfirm(entry.path);
                    }}
                    aria-label={`Añadir ${entry.name}`}
                    className="bg-obsidian hover:bg-accent hover:text-accent-ink text-accent focus-visible:ring-accent/40 h-7 rounded-md px-2.5 text-xs font-medium opacity-0 transition-[opacity,background-color,color] duration-[120ms] group-hover:opacity-100 focus:opacity-100 focus:outline-none focus-visible:ring-2"
                  >
                    + Añadir
                  </button>
                </div>
              );
            })
          ) : (
            <div className="text-fog py-16 text-center font-sans text-sm">Sin carpetas</div>
          )}
        </div>

        <div className="bg-obsidian text-fog flex items-center justify-between p-3 text-xs">
          <span className="text-ash font-sans text-xs">↑↓ navegar · Enter entrar · Esc cerrar</span>

          <div className="flex items-center gap-2">
            {selectedEntry && (
              <button
                type="button"
                onClick={() => void handleConfirm(listing?.current_path)}
                disabled={isSubmitting || !listing}
                className="text-fog hover:text-paper focus-visible:ring-accent/40 rounded-md px-2.5 py-1.5 text-xs transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2"
                title="Añadir la carpeta abierta actualmente"
              >
                Añadir actual
              </button>
            )}

            <button
              type="button"
              onClick={() => void handleConfirm()}
              disabled={isSubmitting || !listing}
              className="bg-accent text-accent-ink focus-visible:ring-accent/40 flex h-9 items-center gap-2 rounded-md px-4 text-sm font-medium transition-[filter] duration-[120ms] hover:brightness-105 focus:outline-none focus-visible:ring-2 disabled:opacity-50"
            >
              <FolderPlus className="h-4 w-4" />
              <span className="max-w-48 truncate">Añadir &ldquo;{targetLabel}&rdquo;</span>
            </button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
