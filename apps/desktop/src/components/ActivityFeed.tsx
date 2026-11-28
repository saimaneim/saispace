import { Activity, Bot, User as UserIcon } from "lucide-react";
import { useMemo } from "react";
import type { ActivityEvent } from "../lib/types";
import { cn, formatTimeAgo } from "../lib/utils";

interface ActivityFeedProps {
  activities: ActivityEvent[];
}

export function ActivityFeed({ activities }: ActivityFeedProps) {
  const sortedActivities = useMemo(() => {
    return [...activities].sort(
      (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    );
  }, [activities]);

  const getActionLabel = (action: string, entity: string) => {
    const entityMap: Record<string, string> = {
      project: "proyecto",
      task: "tarea",
      context_notes: "contexto",
      tech_rule: "regla técnica",
      stack: "stack",
      search: "búsqueda",
      memory: "memoria",
    };
    const translatedEntity = entityMap[entity] ?? entity;

    switch (action) {
      case "create":
        return `creó ${translatedEntity}`;
      case "update":
        return `actualizó ${translatedEntity}`;
      case "delete":
        return `eliminó ${translatedEntity}`;
      case "read":
        return `consultó ${translatedEntity}`;
      default:
        return `${action} ${translatedEntity}`;
    }
  };

  return (
    <section
      aria-label="Actividad"
      className="flex h-full w-full flex-col bg-transparent select-none"
    >
      <div className="bg-carbon flex h-12 flex-shrink-0 items-center px-4">
        <span className="text-paper font-sans text-sm font-semibold tracking-tight">Actividad</span>
      </div>

      <div
        role="feed"
        aria-label="Eventos recientes de actividad"
        className="flex-1 space-y-1.5 overflow-y-auto px-3 pb-3"
      >
        {sortedActivities.map((a) => {
          const isAgent = a.actor.type === "agent";
          const actorName = a.actor.type === "agent" ? a.actor.name : "Usuario";

          return (
            <div
              key={a.id}
              className="bg-obsidian/60 hover:bg-obsidian min-w-0 space-y-1 rounded-lg p-3 transition-colors duration-[120ms]"
            >
              <div className="flex min-w-0 items-center justify-between gap-2">
                <div className="flex min-w-0 flex-1 items-center gap-2">
                  {isAgent ? (
                    <div className="bg-accent/12 text-accent flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md">
                      <Bot className="h-3.5 w-3.5" />
                    </div>
                  ) : (
                    <div className="bg-elevated text-mist flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md">
                      <UserIcon className="h-3.5 w-3.5" />
                    </div>
                  )}

                  <span
                    className={cn(
                      "truncate font-sans text-xs font-medium",
                      isAgent ? "text-accent" : "text-paper",
                    )}
                  >
                    {actorName}
                  </span>
                </div>

                <span className="text-ash flex-shrink-0 font-sans text-xs tabular-nums">
                  {formatTimeAgo(a.created_at)}
                </span>
              </div>

              <div className="text-fog pl-7 font-sans text-xs break-words">
                <span>{getActionLabel(a.action, a.entity)}</span>
              </div>
            </div>
          );
        })}

        {sortedActivities.length === 0 && (
          <div className="text-fog space-y-2 py-16 text-center text-sm">
            <Activity className="text-ash mx-auto mb-2 h-6 w-6" />
            <p>Sin actividad registrada</p>
          </div>
        )}
      </div>
    </section>
  );
}
