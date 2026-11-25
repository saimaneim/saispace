import { BarChart3, ChevronDown, ChevronUp } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { MemoryUsageStats } from "../lib/types";
import { formatTimeAgo } from "../lib/utils";

export interface MemoryUsagePanelProps {
  refreshKey?: string | null;
  onSelectMemory?: (id: string) => void;
}

export function MemoryUsagePanel({ refreshKey, onSelectMemory }: MemoryUsagePanelProps) {
  const [stats, setStats] = useState<MemoryUsageStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(false);

  const load = useCallback(async () => {
    setError(null);
    try {
      setStats(await api.memoryUsageStats());
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshKey;
    load();
  }, [load, refreshKey]);

  const usedCount = stats ? stats.by_project.reduce((sum, item) => sum + item.used, 0) : 0;

  return (
    <section aria-label="Uso de memoria" className="px-6 pb-3 flex-shrink-0">
      <div className="bg-carbon rounded-lg px-4 py-3 space-y-3">
        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-2 min-w-0">
            <BarChart3 className="w-4 h-4 text-fog flex-shrink-0" />
            <h2 className="text-xs font-semibold text-paper">Uso de memoria</h2>
          </div>

          <div className="flex items-center gap-3">
            {stats && (
              <div className="flex items-center gap-2 text-xs">
                <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-obsidian text-fog">
                  <span>Total</span>
                  <span className="text-paper font-medium tabular-nums">{stats.total}</span>
                </span>
                <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-obsidian text-fog">
                  <span>Usadas</span>
                  <span className="text-pulse-green font-medium tabular-nums">{usedCount}</span>
                </span>
                <span
                  className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-obsidian text-fog"
                  title="Memorias con cero consultas (excluye preferencias)"
                >
                  <span>Nunca usadas</span>
                  <span className="text-mist font-medium tabular-nums">{stats.never_used}</span>
                </span>
              </div>
            )}

            <button
              type="button"
              onClick={() => setCollapsed((prev) => !prev)}
              aria-expanded={!collapsed}
              aria-label={collapsed ? "Mostrar detalles de uso" : "Ocultar detalles de uso"}
              className="h-7 w-7 rounded-md flex items-center justify-center text-fog hover:text-paper hover:bg-obsidian transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
            >
              {collapsed ? <ChevronDown className="w-4 h-4" /> : <ChevronUp className="w-4 h-4" />}
            </button>
          </div>
        </div>

        {error && (
          <div className="px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-xs" role="alert">
            {error}
          </div>
        )}

        {!collapsed &&
          (loading && !stats ? (
            <p className="text-xs text-fog py-2">Cargando estadísticas…</p>
          ) : stats ? (
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-3">
              <div className="bg-obsidian rounded-md p-3 flex flex-col gap-2 min-w-0">
                <div className="flex items-center justify-between text-xs text-fog">
                  <h3 className="font-semibold text-mist">Guardados por agente</h3>
                  <div className="flex items-center gap-3 tabular-nums">
                    <span className="w-7 text-right">7 d</span>
                    <span className="w-8 text-right">30 d</span>
                  </div>
                </div>
                {stats.saves_by_agent.length === 0 ? (
                  <p className="text-xs text-fog py-2">Sin guardados recientes de agentes.</p>
                ) : (
                  <ul className="space-y-1 max-h-36 overflow-y-auto">
                    {stats.saves_by_agent.map((agent) => (
                      <li
                        key={agent.actor_name}
                        className="flex items-center justify-between gap-2 text-xs py-0.5"
                      >
                        <span className="text-paper truncate flex-1">{agent.actor_name}</span>
                        <div className="flex items-center gap-3 tabular-nums flex-shrink-0">
                          <span className="w-7 text-right text-mist">{agent.last_7d}</span>
                          <span className="w-8 text-right text-fog">{agent.last_30d}</span>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              <div className="bg-obsidian rounded-md p-3 flex flex-col gap-2 min-w-0">
                <h3 className="text-xs font-semibold text-mist">Más consultadas</h3>
                {stats.most_used.length === 0 ? (
                  <p className="text-xs text-fog py-2">Ninguna memoria consultada todavía.</p>
                ) : (
                  <ul className="space-y-1 max-h-36 overflow-y-auto">
                    {stats.most_used.map((item) => (
                      <li key={item.id}>
                        <button
                          type="button"
                          onClick={() => onSelectMemory?.(item.id)}
                          disabled={!onSelectMemory}
                          className="w-full flex items-center justify-between gap-2 px-1.5 py-1 -mx-1.5 rounded text-left hover:bg-elevated disabled:hover:bg-transparent disabled:cursor-default transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40"
                          title={
                            item.last_used_at
                              ? `Última consulta ${formatTimeAgo(item.last_used_at)}`
                              : undefined
                          }
                        >
                          <span className="text-xs text-paper truncate flex-1">{item.title}</span>
                          <span className="text-xs text-fog truncate max-w-24 flex-shrink-0">
                            {item.project_name ?? "Global"}
                          </span>
                          <span className="text-xs font-medium text-acid-lime tabular-nums flex-shrink-0">
                            {item.use_count}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              <div className="bg-obsidian rounded-md p-3 flex flex-col gap-2 min-w-0">
                <h3 className="text-xs font-semibold text-mist">Uso por proyecto</h3>
                {stats.by_project.length === 0 ? (
                  <p className="text-xs text-fog py-2">Sin memorias registradas todavía.</p>
                ) : (
                  <ul className="space-y-2 max-h-36 overflow-y-auto pr-0.5">
                    {stats.by_project.map((proj) => {
                      const pct =
                        proj.total > 0
                          ? Math.min(100, Math.round((proj.used / proj.total) * 100))
                          : 0;
                      const label = proj.project_name === "global" ? "Global" : proj.project_name;
                      return (
                        <li key={proj.project_name} className="space-y-1">
                          <div className="flex items-center justify-between gap-2 text-xs">
                            <span className="text-paper truncate flex-1">{label}</span>
                            <span className="text-fog tabular-nums flex-shrink-0">
                              {proj.used}/{proj.total}
                            </span>
                          </div>
                          <div className="h-1.5 w-full rounded-full bg-carbon overflow-hidden">
                            <div
                              className="h-full rounded-full bg-acid-lime transition-[width] duration-[120ms]"
                              style={{ width: `${pct}%` }}
                            />
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
            </div>
          ) : null)}
      </div>
    </section>
  );
}
