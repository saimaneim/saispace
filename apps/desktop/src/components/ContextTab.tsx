import {
  BookOpen,
  ChevronRight,
  Compass,
  FileCode,
  FilePlus,
  FileText,
  Folder,
  Link2,
  Scale,
  Search,
  Sparkles,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, formatError } from "../lib/api";
import type { ContextFile, Project } from "../lib/types";
import { cn } from "../lib/utils";
import { Markdown } from "./Markdown";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

/** A file that only points at another one reads as secondary: link icon, dim name, target. */
function linkHint(file: ContextFile): string {
  if (!file.link) return "";
  return file.link.via === "symlink"
    ? `Symlink a ${file.link.target}`
    : `Importa ${file.link.target}`;
}

function FileLabel({ file, label }: { file: ContextFile; label?: string }) {
  const name = label ?? file.name;
  if (!file.link) return <span className="truncate flex-1">{name}</span>;
  return (
    <span className="flex-1 min-w-0 flex items-center gap-1.5" title={linkHint(file)}>
      <span className="truncate italic text-fog">{name}</span>
      <span className="truncate text-[11px] text-ash">→ {file.link.target}</span>
    </span>
  );
}

function FileIcon({ file, icon: Icon }: { file: ContextFile; icon: typeof FileText }) {
  const Shown = file.link ? Link2 : Icon;
  return <Shown className="w-3.5 h-3.5 text-fog flex-shrink-0" />;
}

interface ContextTabProps {
  project: Project;
}

/** Markdown opens rendered; data files (json, toml, yaml) open in the editor. */
const isMarkdown = (path: string) => /\.(md|mdc)$/i.test(path);

interface SkillSubgroup {
  id: string;
  name: string;
  baseDir: string;
  files: { file: ContextFile; relPath: string }[];
}

interface CategorizedContext {
  guidelines: ContextFile[];
  skills: SkillSubgroup[];
  rules: ContextFile[];
  others: ContextFile[];
}

function categorizeContextFiles(files: ContextFile[]): CategorizedContext {
  const guidelines: ContextFile[] = [];
  const rules: ContextFile[] = [];
  const others: ContextFile[] = [];
  const skillsMap = new Map<string, SkillSubgroup>();

  for (const file of files) {
    const normalized = file.path.replace(/\\/g, "/");
    const parts = normalized.split("/");

    if (parts.length === 1) {
      guidelines.push(file);
      continue;
    }

    const skillsIndex = parts.indexOf("skills");
    if (skillsIndex !== -1 && skillsIndex < parts.length - 1) {
      const skillName = parts[skillsIndex + 1];
      const baseDir = parts.slice(0, skillsIndex + 2).join("/");
      const relPath = parts.slice(skillsIndex + 2).join("/") || file.name;

      let subgroup = skillsMap.get(baseDir);
      if (!subgroup) {
        subgroup = {
          id: baseDir,
          name: skillName,
          baseDir,
          files: [],
        };
        skillsMap.set(baseDir, subgroup);
      }
      subgroup.files.push({ file, relPath });
      continue;
    }

    if (parts.includes("rules")) {
      rules.push(file);
      continue;
    }

    others.push(file);
  }

  const priorityRoot = ["AGENTS.md", "CLAUDE.md", "DESIGN.md", "README.md"];
  guidelines.sort((a, b) => {
    const ai = priorityRoot.indexOf(a.name);
    const bi = priorityRoot.indexOf(b.name);
    if (ai !== -1 && bi !== -1) return ai - bi;
    if (ai !== -1) return -1;
    if (bi !== -1) return 1;
    return a.name.localeCompare(b.name);
  });

  const skills = [...skillsMap.values()].sort((a, b) => a.name.localeCompare(b.name));
  for (const skill of skills) {
    skill.files.sort((a, b) => {
      if (a.relPath === "SKILL.md") return -1;
      if (b.relPath === "SKILL.md") return 1;
      return a.relPath.localeCompare(b.relPath);
    });
  }

  rules.sort((a, b) => a.name.localeCompare(b.name));
  others.sort((a, b) => a.path.localeCompare(b.path));

  return { guidelines, skills, rules, others };
}

const MINIMAL_AGENTS_TEMPLATE = `# Directrices para agentes de IA

## Proyecto
Contexto y directrices principales de este proyecto.

## Reglas
- Seguir convenciones del proyecto
- Evitar suposiciones sin verificar
`;

export function ContextTab({ project }: ContextTabProps) {
  const [files, setFiles] = useState<ContextFile[]>([]);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [isLoadingFile, setIsLoadingFile] = useState(false);
  // Set when the selected file could not be read: editing is blocked so a
  // save cannot overwrite it with the previous file's text.
  const [loadFailed, setLoadFailed] = useState(false);
  // Only the latest read may update the editor; an older, slower one would
  // otherwise show (and later save) another file's content under this key.
  const loadSeq = useRef(0);
  const [isSaving, setIsSaving] = useState(false);
  const [isPreview, setIsPreview] = useState(true);
  const [hasSaved, setHasSaved] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [pendingSwitchKey, setPendingSwitchKey] = useState<string | null>(null);
  const [searchFilter, setSearchFilter] = useState("");
  const [openCategories, setOpenCategories] = useState<Set<string>>(new Set(["guidelines"]));
  const [openSkills, setOpenSkills] = useState<Set<string>>(new Set());

  const loadFiles = useCallback(async () => {
    try {
      const list = await api.listContextFiles(project.id);
      setFiles(list);
    } catch {
      setFiles([]);
    }
  }, [project.id]);

  useEffect(() => {
    loadFiles();
  }, [loadFiles]);

  const loadFileContent = useCallback(
    async (path: string) => {
      const seq = ++loadSeq.current;
      setIsLoadingFile(true);
      setLoadFailed(false);
      setErrorMessage(null);
      try {
        const text = await api.readContextFile(project.id, path);
        if (seq !== loadSeq.current) return;
        setContent(text);
        setSavedContent(text);
      } catch (err) {
        if (seq !== loadSeq.current) return;
        setContent("");
        setSavedContent("");
        setLoadFailed(true);
        setErrorMessage(formatError(err));
      } finally {
        if (seq === loadSeq.current) setIsLoadingFile(false);
      }
    },
    [project.id],
  );

  // Open the main guideline (AGENTS.md first) as soon as the list arrives.
  const defaultKey = useMemo(() => {
    const { guidelines, skills, rules, others } = categorizeContextFiles(files);
    return (
      guidelines[0]?.path ?? rules[0]?.path ?? skills[0]?.files[0]?.file.path ?? others[0]?.path
    );
  }, [files]);
  useEffect(() => {
    if (selectedKey === null && defaultKey) {
      setSelectedKey(defaultKey);
      setIsPreview(isMarkdown(defaultKey));
      loadFileContent(defaultKey);
    }
  }, [selectedKey, defaultKey, loadFileContent]);

  const selectItem = useCallback(
    (key: string) => {
      if (key === selectedKey && !loadFailed) return;

      if (content !== savedContent) {
        setPendingSwitchKey(key);
        return;
      }

      setSelectedKey(key);
      setIsPreview(isMarkdown(key));
      loadFileContent(key);
    },
    [selectedKey, loadFailed, content, savedContent, loadFileContent],
  );

  const handleSave = useCallback(async (): Promise<boolean> => {
    if (content === savedContent) return true;
    if (!selectedKey || isSaving || isLoadingFile || loadFailed) return false;
    setIsSaving(true);
    setErrorMessage(null);
    try {
      await api.writeContextFile(project.id, selectedKey, content);
      setSavedContent(content);
      setHasSaved(true);
      setTimeout(() => setHasSaved(false), 2000);
      await loadFiles();
      return true;
    } catch (err) {
      setErrorMessage(formatError(err));
      return false;
    } finally {
      setIsSaving(false);
    }
  }, [
    content,
    savedContent,
    isSaving,
    isLoadingFile,
    loadFailed,
    selectedKey,
    project.id,
    loadFiles,
  ]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        handleSave();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleSave]);

  const switchTo = (target: string) => {
    setPendingSwitchKey(null);
    setSelectedKey(target);
    setIsPreview(isMarkdown(target));
    loadFileContent(target);
  };

  const handleDiscardAndSwitch = () => {
    if (pendingSwitchKey) switchTo(pendingSwitchKey);
  };

  // A failed save keeps the dialog closed but stays on the current file, so
  // the unsaved text is not thrown away.
  const handleSaveAndSwitch = async () => {
    if (!pendingSwitchKey) return;
    const target = pendingSwitchKey;
    if (await handleSave()) {
      switchTo(target);
    } else {
      setPendingSwitchKey(null);
    }
  };

  const handleCreateAgentsMd = async () => {
    setIsSaving(true);
    setErrorMessage(null);
    try {
      await api.writeContextFile(project.id, "AGENTS.md", MINIMAL_AGENTS_TEMPLATE);
      await loadFiles();
      loadSeq.current++;
      setLoadFailed(false);
      setSelectedKey("AGENTS.md");
      setIsPreview(true);
      setContent(MINIMAL_AGENTS_TEMPLATE);
      setSavedContent(MINIMAL_AGENTS_TEMPLATE);
    } catch (err) {
      setErrorMessage(formatError(err));
    } finally {
      setIsSaving(false);
    }
  };

  const hasChanges = content !== savedContent;

  const rawCategorized = useMemo(() => categorizeContextFiles(files), [files]);

  const query = searchFilter.trim().toLowerCase();

  const categorized = useMemo(() => {
    if (!query) return rawCategorized;

    const filterFile = (f: ContextFile) =>
      f.name.toLowerCase().includes(query) || f.path.toLowerCase().includes(query);

    const guidelines = rawCategorized.guidelines.filter(filterFile);
    const rules = rawCategorized.rules.filter(filterFile);
    const others = rawCategorized.others.filter(filterFile);

    const skills: SkillSubgroup[] = [];
    for (const skill of rawCategorized.skills) {
      const matchSkillName = skill.name.toLowerCase().includes(query);
      const filteredFiles = skill.files.filter(
        ({ file, relPath }) =>
          matchSkillName ||
          file.name.toLowerCase().includes(query) ||
          relPath.toLowerCase().includes(query),
      );
      if (filteredFiles.length > 0) {
        skills.push({ ...skill, files: filteredFiles });
      }
    }

    return { guidelines, skills, rules, others };
  }, [rawCategorized, query]);

  const toggleCategory = (key: string) => {
    setOpenCategories((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const toggleSkill = (key: string) => {
    setOpenSkills((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const isCategoryOpen = (key: string) => {
    if (query) return true;
    return openCategories.has(key);
  };

  const isSkillOpen = (key: string) => {
    if (query) return true;
    return openSkills.has(key);
  };

  const currentLabel = selectedKey
    ? (files.find((f) => f.path === selectedKey)?.name ?? selectedKey)
    : "Contexto";
  const selectedFile = files.find((f) => f.path === selectedKey);
  const currentSubtitle = selectedKey ?? "Sin ficheros de contexto";
  const linkTarget = selectedFile?.link
    ? files.find((f) => f.path === selectedFile.link?.target)
    : undefined;

  const rowClass = (selected: boolean) =>
    cn(
      "w-full flex items-center gap-2 px-2.5 h-8 rounded-md text-xs text-left transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
      selected
        ? "bg-elevated text-paper font-medium"
        : "text-mist hover:text-paper hover:bg-obsidian",
    );

  return (
    <div className="flex-1 flex h-full overflow-hidden bg-void select-none">
      <aside className="w-64 flex-shrink-0 bg-carbon flex flex-col h-full overflow-hidden">
        <div className="p-3 pb-2 flex-shrink-0">
          <div className="relative">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-fog pointer-events-none" />
            <input
              type="text"
              value={searchFilter}
              onChange={(e) => setSearchFilter(e.target.value)}
              placeholder="Filtrar contexto…"
              aria-label="Filtrar contexto"
              className="w-full h-8 pl-8 pr-2.5 text-xs bg-obsidian text-paper placeholder:text-fog rounded-md focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 transition-colors duration-[120ms]"
            />
          </div>
        </div>

        <nav
          className="flex-1 overflow-y-auto px-3 pb-3 space-y-1"
          aria-label="Ficheros de contexto"
        >
          {categorized.guidelines.length > 0 && (
            <div className="pt-1">
              <button
                type="button"
                onClick={() => toggleCategory("guidelines")}
                aria-expanded={isCategoryOpen("guidelines")}
                className="w-full flex items-center gap-1.5 px-1.5 h-7 rounded-md text-xs font-semibold text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
              >
                <ChevronRight
                  className={cn(
                    "w-3.5 h-3.5 flex-shrink-0 text-ash transition-transform duration-[120ms]",
                    isCategoryOpen("guidelines") && "rotate-90 text-paper",
                  )}
                />
                <Compass className="w-3.5 h-3.5 text-fog flex-shrink-0" />
                <span className="truncate flex-1 text-left">Directrices</span>
                <span className="tabular-nums text-[11px] text-ash font-normal">
                  {categorized.guidelines.length}
                </span>
              </button>

              {isCategoryOpen("guidelines") && (
                <div className="mt-0.5 space-y-0.5 pl-3">
                  {categorized.guidelines.map((f) => (
                    <button
                      key={f.path}
                      type="button"
                      onClick={() => selectItem(f.path)}
                      title={f.path}
                      className={rowClass(selectedKey === f.path)}
                    >
                      <FileIcon file={f} icon={BookOpen} />
                      <FileLabel file={f} />
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {categorized.skills.length > 0 && (
            <div className="pt-1">
              <button
                type="button"
                onClick={() => toggleCategory("skills")}
                aria-expanded={isCategoryOpen("skills")}
                className="w-full flex items-center gap-1.5 px-1.5 h-7 rounded-md text-xs font-semibold text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
              >
                <ChevronRight
                  className={cn(
                    "w-3.5 h-3.5 flex-shrink-0 text-ash transition-transform duration-[120ms]",
                    isCategoryOpen("skills") && "rotate-90 text-paper",
                  )}
                />
                <Sparkles className="w-3.5 h-3.5 text-fog flex-shrink-0" />
                <span className="truncate flex-1 text-left">Skills</span>
                <span className="tabular-nums text-[11px] text-ash font-normal">
                  {categorized.skills.length}
                </span>
              </button>

              {isCategoryOpen("skills") && (
                <div className="mt-0.5 space-y-0.5 pl-2">
                  {categorized.skills.map((skill) => {
                    const hasSelected = skill.files.some((f) => f.file.path === selectedKey);
                    const open = isSkillOpen(skill.id) || hasSelected;
                    return (
                      <div key={skill.id} className="space-y-0.5">
                        <button
                          type="button"
                          onClick={() => toggleSkill(skill.id)}
                          aria-expanded={open}
                          title={skill.baseDir}
                          className="w-full flex items-center gap-1.5 px-2 h-7 rounded-md text-xs text-mist hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                        >
                          <ChevronRight
                            className={cn(
                              "w-3 h-3 flex-shrink-0 text-ash transition-transform duration-[120ms]",
                              open && "rotate-90 text-paper",
                            )}
                          />
                          <Folder className="w-3.5 h-3.5 text-fog flex-shrink-0" />
                          <span className="truncate flex-1 text-left">{skill.name}</span>
                          {skill.files.length > 1 && (
                            <span className="tabular-nums text-[11px] text-ash">
                              {skill.files.length}
                            </span>
                          )}
                        </button>

                        {open && (
                          <div className="space-y-0.5 pl-4">
                            {skill.files.map(({ file, relPath }) => (
                              <button
                                key={file.path}
                                type="button"
                                onClick={() => selectItem(file.path)}
                                title={file.path}
                                className={rowClass(selectedKey === file.path)}
                              >
                                <FileLabel file={file} label={relPath} />
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {categorized.rules.length > 0 && (
            <div className="pt-1">
              <button
                type="button"
                onClick={() => toggleCategory("rules")}
                aria-expanded={isCategoryOpen("rules")}
                className="w-full flex items-center gap-1.5 px-1.5 h-7 rounded-md text-xs font-semibold text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
              >
                <ChevronRight
                  className={cn(
                    "w-3.5 h-3.5 flex-shrink-0 text-ash transition-transform duration-[120ms]",
                    isCategoryOpen("rules") && "rotate-90 text-paper",
                  )}
                />
                <Scale className="w-3.5 h-3.5 text-fog flex-shrink-0" />
                <span className="truncate flex-1 text-left">Reglas</span>
                <span className="tabular-nums text-[11px] text-ash font-normal">
                  {categorized.rules.length}
                </span>
              </button>

              {isCategoryOpen("rules") && (
                <div className="mt-0.5 space-y-0.5 pl-3">
                  {categorized.rules.map((f) => (
                    <button
                      key={f.path}
                      type="button"
                      onClick={() => selectItem(f.path)}
                      title={f.path}
                      className={rowClass(selectedKey === f.path)}
                    >
                      <FileIcon file={f} icon={FileCode} />
                      <FileLabel file={f} />
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {categorized.others.length > 0 && (
            <div className="pt-1">
              <button
                type="button"
                onClick={() => toggleCategory("others")}
                aria-expanded={isCategoryOpen("others")}
                className="w-full flex items-center gap-1.5 px-1.5 h-7 rounded-md text-xs font-semibold text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
              >
                <ChevronRight
                  className={cn(
                    "w-3.5 h-3.5 flex-shrink-0 text-ash transition-transform duration-[120ms]",
                    isCategoryOpen("others") && "rotate-90 text-paper",
                  )}
                />
                <Folder className="w-3.5 h-3.5 text-fog flex-shrink-0" />
                <span className="truncate flex-1 text-left">Otros</span>
                <span className="tabular-nums text-[11px] text-ash font-normal">
                  {categorized.others.length}
                </span>
              </button>

              {isCategoryOpen("others") && (
                <div className="mt-0.5 space-y-0.5 pl-3">
                  {categorized.others.map((f) => (
                    <button
                      key={f.path}
                      type="button"
                      onClick={() => selectItem(f.path)}
                      title={f.path}
                      className={rowClass(selectedKey === f.path)}
                    >
                      <FileIcon file={f} icon={FileText} />
                      <FileLabel file={f} />
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {files.length === 0 && (
            <div className="py-6 px-1 text-xs text-fog space-y-3">
              <p>Este proyecto no tiene ficheros de contexto.</p>
              <Button
                variant="secondary"
                onClick={handleCreateAgentsMd}
                disabled={isSaving}
                className="w-full h-8 text-xs"
              >
                <FilePlus className="w-3.5 h-3.5 mr-1.5" />
                Crear AGENTS.md
              </Button>
            </div>
          )}
        </nav>
      </aside>

      <section className="flex-1 flex flex-col h-full min-w-0 overflow-hidden bg-void">
        <div className="h-14 px-5 flex items-center gap-4 flex-shrink-0">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold text-paper truncate">{currentLabel}</h2>
              {hasChanges && (
                <span
                  className="w-2 h-2 rounded-full bg-status-amber flex-shrink-0"
                  title="Cambios sin guardar"
                />
              )}
            </div>
            {selectedFile?.link ? (
              <p className="text-xs text-fog flex items-center gap-1.5 min-w-0">
                <Link2 className="w-3 h-3 flex-shrink-0" />
                <span className="truncate">
                  {selectedFile.link.via === "symlink" ? "Symlink a" : "Solo importa"}{" "}
                  {linkTarget ? (
                    <button
                      type="button"
                      onClick={() => selectItem(linkTarget.path)}
                      className="text-mist underline underline-offset-2 hover:text-paper focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40 rounded-sm"
                    >
                      {selectedFile.link.target}
                    </button>
                  ) : (
                    <span className="text-mist">{selectedFile.link.target}</span>
                  )}
                  {selectedFile.link.via === "symlink" ? ": editar aquí cambia el original" : ""}
                </span>
              </p>
            ) : (
              <p className="text-xs text-fog truncate">{currentSubtitle}</p>
            )}
          </div>

          <div
            className="flex items-center gap-1 bg-obsidian p-1 rounded-lg flex-shrink-0"
            role="tablist"
          >
            {[
              { id: false, label: "Editar" },
              { id: true, label: "Vista previa" },
            ].map(({ id, label }) => (
              <button
                key={label}
                type="button"
                role="tab"
                aria-selected={isPreview === id}
                onClick={() => setIsPreview(id)}
                className={cn(
                  "h-7 px-3 rounded-md text-xs transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                  isPreview === id
                    ? "bg-elevated text-paper font-medium"
                    : "text-fog hover:text-mist",
                )}
              >
                {label}
              </button>
            ))}
          </div>

          <Button
            onClick={handleSave}
            disabled={!hasChanges || isSaving || loadFailed}
            variant={hasChanges ? "default" : "secondary"}
            title="Guardar (Ctrl+S)"
            className="flex-shrink-0 min-w-24"
          >
            {hasSaved ? "Guardado" : isSaving ? "Guardando…" : "Guardar"}
          </Button>
        </div>

        {errorMessage && (
          <div
            className="mx-5 mb-2 px-3 py-2 rounded-md text-xs bg-coral-red/10 text-coral-red"
            role="alert"
          >
            {errorMessage}
          </div>
        )}

        <div className="flex-1 px-5 pb-5 overflow-y-auto">
          {selectedKey === null ? (
            <div className="py-20 text-center text-xs text-fog">
              Sin ficheros de contexto. Crea un AGENTS.md desde la barra lateral.
            </div>
          ) : isLoadingFile ? (
            <div className="py-20 text-center text-xs text-fog">Cargando…</div>
          ) : loadFailed ? (
            <div className="py-20 text-center text-xs text-fog">
              No se pudo leer el fichero. Vuelve a seleccionarlo para reintentar.
            </div>
          ) : isPreview ? (
            <div className="max-w-3xl space-y-2">
              {content.trim() ? (
                <Markdown content={content} />
              ) : (
                <div className="py-16 text-center text-xs text-fog space-y-2">
                  <FileText className="w-6 h-6 mx-auto text-ash mb-2" />
                  <p>Documento vacío</p>
                </div>
              )}
            </div>
          ) : (
            <textarea
              name="context_editor"
              spellCheck={false}
              aria-label="Editor de contenido"
              value={content}
              onChange={(e) => setContent(e.target.value)}
              placeholder="Escribe aquí las directrices y el contexto para los agentes…"
              className="w-full h-full min-h-96 font-mono text-sm leading-relaxed p-4 bg-carbon rounded-lg text-paper placeholder:text-fog resize-none focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
            />
          )}
        </div>
      </section>

      <Dialog open={Boolean(pendingSwitchKey)} onOpenChange={() => setPendingSwitchKey(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Cambios sin guardar</DialogTitle>
            <DialogDescription>
              Hay cambios sin guardar en este fichero. ¿Deseas guardarlos antes de cambiar o
              descartarlos?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setPendingSwitchKey(null)} autoFocus>
              Cancelar
            </Button>
            <Button variant="destructive" onClick={handleDiscardAndSwitch}>
              Descartar
            </Button>
            <Button variant="default" onClick={handleSaveAndSwitch}>
              Guardar y cambiar
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
