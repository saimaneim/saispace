import { Activity, Bot, User as UserIcon } from "lucide-react";
import { useMemo } from "react";
import type { ActivityEvent } from "../lib/types";
import { formatTimeAgo } from "../lib/utils";

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
      className="w-full flex flex-col h-full select-none bg-transparent"
    >
      <div className="h-12 px-4 flex items-center bg-obsidian/40 flex-shrink-0">
        <span className="font-semibold text-sm text-paper tracking-tight font-sans">Actividad</span>
      </div>

      <div
        role="feed"
        aria-label="Eventos recientes de actividad"
        className="flex-1 overflow-y-auto p-3 space-y-1.5"
      >
        {sortedActivities.map((a) => {
          const isAgent = a.actor.type === "agent";
          const actorName = a.actor.type === "agent" ? a.actor.name : "Usuario";

          return (
            <div
              key={a.id}
              className="p-3 rounded-md bg-obsidian/40 hover:bg-obsidian/70 transition-colors duration-[120ms] space-y-1"
            >
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 min-w-0">
                  {isAgent ? (
                    <div className="w-5 h-5 rounded-md bg-acid-lime/10 text-acid-lime flex items-center justify-center flex-shrink-0">
                      <Bot className="w-3.5 h-3.5" />
                    </div>
                  ) : (
                    <div className="w-5 h-5 rounded-md bg-white/[0.06] text-mist flex items-center justify-center flex-shrink-0">
                      <UserIcon className="w-3.5 h-3.5" />
                    </div>
                  )}

                  <span
                    className={`truncate font-sans text-xs font-medium ${
                      isAgent ? "text-acid-lime" : "text-paper"
                    }`}
                  >
                    {actorName}
                  </span>
                </div>

                <span className="font-sans text-xs text-ash flex-shrink-0 tabular-nums">
                  {formatTimeAgo(a.created_at)}
                </span>
              </div>

              <div className="text-xs text-fog pl-7 font-sans">
                <span>{getActionLabel(a.action, a.entity)}</span>
              </div>
            </div>
          );
        })}

        {sortedActivities.length === 0 && (
          <div className="py-16 text-center text-sm text-fog space-y-2">
            <Activity className="w-6 h-6 mx-auto text-ash mb-2" />
            <p>Sin actividad registrada</p>
          </div>
        )}
      </div>
    </section>
  );
}
