import { FolderCheck } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { OrphanGroup, Project } from "../lib/types";
import { formatTimeAgo } from "../lib/utils";
import { Button } from "./ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

export interface OrphanMemoriesProps {
  onLinked?: () => void;
}

export function OrphanMemories({ onLinked }: OrphanMemoriesProps) {
  const [groups, setGroups] = useState<OrphanGroup[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedProject, setSelectedProject] = useState<Record<string, string>>({});
  const [linking, setLinking] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [fetchedGroups, fetchedProjects] = await Promise.all([
        api.listOrphanMemoryGroups(),
        api.listProjects(),
      ]);
      setGroups(fetchedGroups);
      setProjects(fetchedProjects);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleLink = async (projectName: string) => {
    const projectId = selectedProject[projectName];
    if (!projectId) return;

    setLinking((prev) => ({ ...prev, [projectName]: true }));
    setError(null);
    try {
      await api.linkOrphanMemories(projectName, projectId);
      await load();
      onLinked?.();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLinking((prev) => ({ ...prev, [projectName]: false }));
    }
  };

  if (loading && groups.length === 0) {
    return (
      <section className="flex-1 min-w-0 flex flex-col px-7 pt-5 pb-6 space-y-3">
        <div className="h-6 w-48 rounded-md bg-carbon" />
        <div className="h-16 rounded-xl bg-carbon" />
        <div className="h-16 rounded-xl bg-carbon" />
      </section>
    );
  }

  if (groups.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-2 text-center p-6">
        <FolderCheck className="w-7 h-7 text-ash" />
        <p className="text-sm font-medium text-paper">Todas las memorias tienen proyecto</p>
      </div>
    );
  }

  return (
    <section className="flex-1 min-w-0 flex flex-col px-7 pt-5 pb-6 overflow-y-auto">
      <div className="flex items-center justify-between pb-4">
        <h2 className="text-lg font-semibold text-paper tracking-tight">Memorias sin proyecto</h2>
        <span className="text-xs text-fog tabular-nums">
          {groups.length} {groups.length === 1 ? "grupo" : "grupos"}
        </span>
      </div>

      {error && (
        <div
          className="mb-3 px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm"
          role="alert"
        >
          {error}
        </div>
      )}

      <ul className="space-y-2">
        {groups.map((group) => {
          const isLinking = Boolean(linking[group.project_name]);
          const currentProjectId = selectedProject[group.project_name] ?? "";

          return (
            <li
              key={group.project_name}
              className="bg-carbon rounded-xl p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3"
            >
              <div className="min-w-0 flex-1">
                <span className="block text-sm font-medium text-paper truncate">
                  {group.project_name}
                </span>
                <span className="block text-xs text-fog mt-0.5 tabular-nums">
                  {group.count} {group.count === 1 ? "memoria" : "memorias"} ·{" "}
                  {formatTimeAgo(group.last_updated)}
                </span>
              </div>
              <div className="flex items-center gap-2 flex-shrink-0">
                <Select
                  value={currentProjectId}
                  onValueChange={(value) =>
                    setSelectedProject((prev) => ({
                      ...prev,
                      [group.project_name]: value,
                    }))
                  }
                >
                  <SelectTrigger
                    aria-label={`Vincular ${group.project_name} a un proyecto`}
                    className="h-8 w-44 min-w-0 text-xs"
                  >
                    <SelectValue placeholder="Vincular a…" />
                  </SelectTrigger>
                  <SelectContent>
                    {projects.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Button
                  variant="default"
                  size="sm"
                  disabled={!currentProjectId || isLinking}
                  onClick={() => handleLink(group.project_name)}
                  className="h-8 text-xs"
                >
                  {isLinking ? "Vinculando…" : "Vincular"}
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
