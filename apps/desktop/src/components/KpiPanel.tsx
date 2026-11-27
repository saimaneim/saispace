import { ChevronDown, ChevronUp, TrendingUp } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { KpiPoint, KpiSeries } from "../lib/types";
import { cn } from "../lib/utils";

export interface KpiPanelProps {
  scope: string;
  refreshKey?: string | null;
  defaultCollapsed?: boolean;
}

const LEARNING_METRICS = new Set(["corrections_7d", "stop_blocks_7d", "lesson_conversion_7d"]);

const DAY_RANGES = [7, 30, 90] as const;
type DayRange = (typeof DAY_RANGES)[number];

const INTEGER_FORMAT = new Intl.NumberFormat("es-ES", { maximumFractionDigits: 0 });
const ONE_DECIMAL_FORMAT = new Intl.NumberFormat("es-ES", { maximumFractionDigits: 1 });
const TWO_DECIMALS_FORMAT = new Intl.NumberFormat("es-ES", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function formatMetricValue(value: number, unit: string): string {
  switch (unit) {
    case "ratio":
      return `${ONE_DECIMAL_FORMAT.format(value * 100)}%`;
    case "bytes": {
      const abs = Math.abs(value);
      if (abs < 1024) return `${INTEGER_FORMAT.format(value)} B`;
      if (abs < 1024 * 1024) return `${ONE_DECIMAL_FORMAT.format(value / 1024)} KB`;
      return `${ONE_DECIMAL_FORMAT.format(value / (1024 * 1024))} MB`;
    }
    case "links_per_memory":
      return TWO_DECIMALS_FORMAT.format(value);
    default:
      return INTEGER_FORMAT.format(Math.round(value));
  }
}

function formatMetricDelta(delta: number, unit: string): string {
  const zeroText = formatMetricValue(0, unit);
  const absText = formatMetricValue(Math.abs(delta), unit);
  if (Math.abs(delta) < 1e-9 || absText === zeroText) {
    return zeroText;
  }
  const sign = delta > 0 ? "+" : "-";
  return `${sign}${absText}`;
}

function getDeltaToneClass(delta: number | null, higherIsBetter: boolean | null): string {
  if (delta === null || Math.abs(delta) < 1e-9 || higherIsBetter === null) {
    return "text-fog";
  }
  const improved = higherIsBetter ? delta > 0 : delta < 0;
  return improved ? "text-pulse-green" : "text-coral-red";
}

function parseIsoDayUtc(day: string): number {
  const [year, month, date] = day.split("-").map(Number);
  return Date.UTC(year, (month || 1) - 1, date || 1);
}

function dayDiffBetween(startDay: string, endDay: string): number {
  return Math.round((parseIsoDayUtc(endDay) - parseIsoDayUtc(startDay)) / 86_400_000);
}

function subtractDaysFromIsoDay(day: string, days: number): string {
  return new Date(parseIsoDayUtc(day) - days * 86_400_000).toISOString().slice(0, 10);
}

function sortPoints(points: KpiPoint[]): KpiPoint[] {
  return [...points].sort((a, b) => a.day.localeCompare(b.day));
}

function computeSeriesSummary(points: KpiPoint[]) {
  const sorted = sortPoints(points);
  if (sorted.length === 0) {
    return { sorted, current: null, delta7d: null, deltaFirst: null };
  }
  const current = sorted[sorted.length - 1];
  if (sorted.length < 2) {
    return { sorted, current, delta7d: null, deltaFirst: null };
  }
  const first = sorted[0];
  const deltaFirst = current.value - first.value;
  const target7d = subtractDaysFromIsoDay(current.day, 7);
  let baseline7d: KpiPoint | null = null;
  for (let i = sorted.length - 2; i >= 0; i--) {
    if (sorted[i].day <= target7d) {
      baseline7d = sorted[i];
      break;
    }
  }
  if (!baseline7d && dayDiffBetween(first.day, current.day) >= 6) {
    baseline7d = first;
  }
  const delta7d = baseline7d ? current.value - baseline7d.value : null;
  return { sorted, current, delta7d, deltaFirst };
}

function Sparkline({
  points,
  toneClass,
  label,
}: {
  points: KpiPoint[];
  toneClass: string;
  label: string;
}) {
  const values = points.map((p) => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min;
  const coords = points
    .map((point, index) => {
      const x = 2 + (index / Math.max(1, points.length - 1)) * 116;
      const y = span === 0 ? 14 : 24 - ((point.value - min) / span) * 20;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  return (
    <svg
      viewBox="0 0 120 28"
      preserveAspectRatio="none"
      role="img"
      aria-label={`Tendencia de ${label}`}
      className={cn("w-full h-7 flex-shrink-0", toneClass)}
    >
      <polyline
        fill="none"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
        points={coords}
      />
    </svg>
  );
}

function KpiCard({ series }: { series: KpiSeries }) {
  const { sorted, current, delta7d, deltaFirst } = computeSeriesSummary(series.points);
  const tone7d = getDeltaToneClass(delta7d, series.higher_is_better);
  const toneFirst = getDeltaToneClass(deltaFirst, series.higher_is_better);
  const sparklineTone = getDeltaToneClass(delta7d ?? deltaFirst, series.higher_is_better);

  return (
    <article
      className="bg-obsidian rounded-md p-3 flex flex-col gap-1.5 min-w-0 overflow-hidden"
      aria-label={series.label}
    >
      <span className="text-xs text-fog truncate" title={series.label}>
        {series.label}
      </span>

      <span className="text-base font-semibold text-paper tabular-nums truncate">
        {current ? formatMetricValue(current.value, series.unit) : "—"}
      </span>

      {sorted.length >= 2 ? (
        <>
          <Sparkline points={sorted} toneClass={sparklineTone} label={series.label} />
          <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5 text-xs tabular-nums min-w-0">
            <span className={cn("truncate min-w-0", tone7d)} data-delta-kind="7d">
              7d {delta7d !== null ? formatMetricDelta(delta7d, series.unit) : "—"}
            </span>
            <span className={cn("truncate min-w-0", toneFirst)} data-delta-kind="first">
              Inicio {deltaFirst !== null ? formatMetricDelta(deltaFirst, series.unit) : "—"}
            </span>
          </div>
        </>
      ) : (
        <p className="text-xs text-ash truncate">Sin histórico suficiente</p>
      )}
    </article>
  );
}

function KpiGroup({ title, items }: { title: string; items: KpiSeries[] }) {
  if (items.length === 0) return null;
  return (
    <div className="space-y-2 min-w-0">
      <h3 className="text-xs font-semibold text-fog px-0.5">{title}</h3>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 xl:grid-cols-5 gap-2.5 min-w-0">
        {items.map((series) => (
          <KpiCard key={series.name} series={series} />
        ))}
      </div>
    </div>
  );
}

export function KpiPanel({ scope, refreshKey, defaultCollapsed = false }: KpiPanelProps) {
  const [days, setDays] = useState<DayRange>(30);
  const [metrics, setMetrics] = useState<KpiSeries[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(defaultCollapsed);

  const load = useCallback(async () => {
    setError(null);
    try {
      const history = await api.kpiHistory(days, scope);
      setMetrics(history.metrics);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setLoading(false);
    }
  }, [days, scope]);

  useEffect(() => {
    void refreshKey;
    load();
  }, [load, refreshKey]);

  const hasAnyHistory = metrics.some((series) => series.points.length >= 2);
  const learningMetrics = metrics.filter((series) => LEARNING_METRICS.has(series.name));
  const memoryMetrics = metrics.filter((series) => !LEARNING_METRICS.has(series.name));

  return (
    <section aria-label="Evolución y KPI" className="px-6 pb-3 flex-shrink-0">
      <div className="bg-carbon rounded-lg px-4 py-3 space-y-3">
        <div className="flex items-center justify-between gap-4 min-w-0">
          <div className="flex items-center gap-2 min-w-0">
            <TrendingUp className="w-4 h-4 text-fog flex-shrink-0" />
            <h2 className="text-xs font-semibold text-paper truncate">Evolución y KPI</h2>
          </div>

          <div className="flex items-center gap-2 flex-shrink-0">
            <fieldset
              className="flex items-center gap-1 bg-obsidian p-0.5 rounded-md"
              aria-label="Rango de días"
            >
              {DAY_RANGES.map((option) => (
                <button
                  key={option}
                  type="button"
                  aria-pressed={days === option}
                  onClick={() => setDays(option)}
                  className={cn(
                    "h-6 px-2 text-xs rounded tabular-nums transition-colors duration-[120ms] focus:outline-none focus-visible:ring-2 focus-visible:ring-acid-lime/40",
                    days === option
                      ? "bg-elevated text-paper font-medium"
                      : "text-fog hover:text-mist",
                  )}
                >
                  {`${option} d`}
                </button>
              ))}
            </fieldset>

            <button
              type="button"
              onClick={() => setCollapsed((prev) => !prev)}
              aria-expanded={!collapsed}
              aria-label={collapsed ? "Mostrar indicadores KPI" : "Ocultar indicadores KPI"}
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
          (loading && metrics.length === 0 ? (
            <p className="text-xs text-fog py-2">Cargando indicadores…</p>
          ) : !hasAnyHistory ? (
            <p className="text-xs text-fog py-2">
              Aún no hay histórico: se toma una foto al día al abrir una sesión de agente
            </p>
          ) : (
            <div className="space-y-3 max-h-72 overflow-y-auto pr-0.5 min-w-0">
              <KpiGroup title="Aprendizaje" items={learningMetrics} />
              <KpiGroup title="Memoria" items={memoryMetrics} />
            </div>
          ))}
      </div>
    </section>
  );
}
