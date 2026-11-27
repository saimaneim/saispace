import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { KpiPanel } from "../components/KpiPanel";
import type { KpiHistory } from "../lib/types";
import { installFakeBackend, makeProject } from "./fakeBackend";
import { renderApp } from "./renderApp";

function daysAgo(n: number): string {
  const date = new Date(Date.UTC(2026, 10, 26));
  date.setUTCDate(date.getUTCDate() - n);
  return date.toISOString().slice(0, 10);
}

describe("KpiPanel", () => {
  it("renderiza métricas mockeadas, agrupa en Aprendizaje/Memoria, formatea unidades y colorea deltas según higher_is_better", async () => {
    const history: KpiHistory = {
      metrics: [
        {
          name: "corrections_7d",
          label: "Correcciones (7d)",
          unit: "count",
          higher_is_better: false,
          points: [
            { day: daysAgo(14), value: 2 },
            { day: daysAgo(7), value: 3 },
            { day: daysAgo(0), value: 5 },
          ],
        },
        {
          name: "lesson_conversion_7d",
          label: "Conversión a lección (7d)",
          unit: "ratio",
          higher_is_better: true,
          points: [
            { day: daysAgo(14), value: 0.4 },
            { day: daysAgo(7), value: 0.55 },
            { day: daysAgo(0), value: 0.8 },
          ],
        },
        {
          name: "memories_active",
          label: "Memorias activas",
          unit: "count",
          higher_is_better: null,
          points: [
            { day: daysAgo(14), value: 10 },
            { day: daysAgo(7), value: 18 },
            { day: daysAgo(0), value: 27 },
          ],
        },
        {
          name: "duplicate_pairs",
          label: "Pares duplicados",
          unit: "count",
          higher_is_better: false,
          points: [
            { day: daysAgo(14), value: 5 },
            { day: daysAgo(7), value: 3 },
            { day: daysAgo(0), value: 1 },
          ],
        },
        {
          name: "reuse_rate_30d",
          label: "Tasa de reuso (30d)",
          unit: "ratio",
          higher_is_better: true,
          points: [
            { day: daysAgo(14), value: 0.7 },
            { day: daysAgo(7), value: 0.6 },
            { day: daysAgo(0), value: 0.45 },
          ],
        },
        {
          name: "index_bytes",
          label: "Tamaño del índice",
          unit: "bytes",
          higher_is_better: false,
          points: [
            { day: daysAgo(14), value: 2560 },
            { day: daysAgo(7), value: 2458 },
            { day: daysAgo(0), value: 2355 },
          ],
        },
        {
          name: "link_density",
          label: "Densidad de enlaces",
          unit: "links_per_memory",
          higher_is_better: true,
          points: [
            { day: daysAgo(14), value: 1.0 },
            { day: daysAgo(7), value: 1.0 },
            { day: daysAgo(0), value: 1.0 },
          ],
        },
      ],
    };

    const backend = installFakeBackend({ kpiHistory: history });
    const user = userEvent.setup();
    render(<KpiPanel scope="global" />);

    expect(await screen.findByRole("heading", { name: "Aprendizaje" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Memoria" })).toBeInTheDocument();

    const correctionsCard = screen.getByRole("article", { name: "Correcciones (7d)" });
    expect(within(correctionsCard).getByText("5")).toBeInTheDocument();
    const corr7d = within(correctionsCard).getByText("7d +2");
    const corrFirst = within(correctionsCard).getByText("Inicio +3");
    expect(corr7d).toHaveClass("text-coral-red");
    expect(corrFirst).toHaveClass("text-coral-red");

    const conversionCard = screen.getByRole("article", { name: "Conversión a lección (7d)" });
    expect(within(conversionCard).getByText("80%")).toBeInTheDocument();
    expect(within(conversionCard).getByText("7d +25%")).toHaveClass("text-pulse-green");
    expect(within(conversionCard).getByText("Inicio +40%")).toHaveClass("text-pulse-green");

    const volumeCard = screen.getByRole("article", { name: "Memorias activas" });
    expect(within(volumeCard).getByText("27")).toBeInTheDocument();
    const vol7d = within(volumeCard).getByText("7d +9");
    const volFirst = within(volumeCard).getByText("Inicio +17");
    expect(vol7d).toHaveClass("text-fog");
    expect(volFirst).toHaveClass("text-fog");
    expect(vol7d).not.toHaveClass("text-pulse-green", "text-coral-red");

    const dupCard = screen.getByRole("article", { name: "Pares duplicados" });
    expect(within(dupCard).getByText("7d -2")).toHaveClass("text-pulse-green");
    expect(within(dupCard).getByText("Inicio -4")).toHaveClass("text-pulse-green");

    const reuseCard = screen.getByRole("article", { name: "Tasa de reuso (30d)" });
    expect(within(reuseCard).getByText("45%")).toBeInTheDocument();
    expect(within(reuseCard).getByText("7d -15%")).toHaveClass("text-coral-red");

    const bytesCard = screen.getByRole("article", { name: "Tamaño del índice" });
    expect(within(bytesCard).getByText("2,3 KB")).toBeInTheDocument();
    expect(within(bytesCard).getByText("7d -103 B")).toHaveClass("text-pulse-green");

    const densityCard = screen.getByRole("article", { name: "Densidad de enlaces" });
    expect(within(densityCard).getByText("1,00")).toBeInTheDocument();
    const flatPolyline = within(densityCard).getByRole("img").querySelector("polyline");
    expect(flatPolyline?.getAttribute("points")).not.toContain("NaN");

    await user.click(screen.getByRole("button", { name: "90 d" }));
    expect(backend.callsTo("kpi_history").at(-1)?.args).toEqual({ days: 90, scope: "global" });
  });

  it("muestra el estado vacío cuando todas las métricas tienen menos de 2 puntos", async () => {
    installFakeBackend({
      kpiHistory: {
        metrics: [
          {
            name: "memories_active",
            label: "Memorias activas",
            unit: "count",
            higher_is_better: null,
            points: [{ day: daysAgo(0), value: 12 }],
          },
        ],
      },
    });

    render(<KpiPanel scope="global" />);

    expect(
      await screen.findByText(
        "Aún no hay histórico: se toma una foto al día al abrir una sesión de agente",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("article", { name: "Memorias activas" })).not.toBeInTheDocument();
  });

  it("se integra en la vista Global y en la pestaña Memoria del proyecto con el scope correcto", async () => {
    const project = makeProject({ id: "proj-1", name: "global", path: "/home/test/global" });
    const { backend, user } = await renderApp({ projects: [project] });

    await user.click(await screen.findByRole("tab", { name: /Memoria/ }));
    expect(await screen.findByRole("heading", { name: "Evolución y KPI" })).toBeInTheDocument();
    expect(backend.callsTo("kpi_history").some((c) => c.args?.scope === "proj-1")).toBe(true);
    expect(backend.callsTo("kpi_history").some((c) => c.args?.scope === "global")).toBe(false);

    await user.click(screen.getByRole("button", { name: "Mostrar indicadores KPI" }));
    expect(await screen.findByRole("article", { name: "Tamaño del índice" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Global" }));
    expect(
      await screen.findByRole("article", { name: "Conversión a lección (7d)" }),
    ).toBeInTheDocument();
    expect(backend.callsTo("kpi_history").some((c) => c.args?.scope === "global")).toBe(true);
  });
});
