import { describe, expect, it } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import { makeGoal, makeProject, makeTask, makeTimelineItem } from "./fakeBackend";
import { renderApp } from "./renderApp";

const project = makeProject({ id: "proj-history", name: "facturas", path: "/home/test/facturas" });
const DAY = 24 * 60 * 60 * 1000;
const yesterday = new Date(Date.now() - DAY).toISOString();

const gitHistory = {
  [project.id]: [
    makeTimelineItem({
      kind: "commit",
      at: yesterday,
      label: "Nuevo",
      title: "Exportar facturas en PDF",
      goal: "Exportar facturas",
      pr: 12,
      hash: "a1b2c3d",
      detail: "Genera el PDF con Typst.",
      files: ["src/export.rs", "src/pdf.rs"],
      link: "https://github.com/acme/facturas/pull/12",
    }),
    makeTimelineItem({
      kind: "release",
      at: "2025-03-10T12:00:00Z",
      label: "Versión",
      title: "v0.1.0",
      link: "https://github.com/acme/facturas/releases/tag/v0.1.0",
    }),
    makeTimelineItem({
      kind: "commit",
      at: "2025-03-10T09:00:00Z",
      label: "Arreglo",
      title: "Redondear los totales",
      hash: "9f8e7d6",
    }),
  ],
};

describe("flujo: Historial", () => {
  it("lo hecho en Plan aparece como hito de hoy junto a los PR, con detalle técnico al desplegar", async () => {
    const goal = makeGoal({
      id: "goal-export",
      project_id: project.id,
      title: "Exportar facturas",
    });
    const task = makeTask({
      id: "task-pdf",
      project_id: project.id,
      goal_id: goal.id,
      title: "Generar el PDF",
    });
    const { backend, user } = await renderApp({
      projects: [project],
      goals: [goal],
      tasks: [task],
      gitHistory,
    });

    const article = await screen.findByRole("article", { name: goal.title });
    await user.click(within(article).getByRole("checkbox", { name: task.title }));
    await waitFor(() => expect(backend.tasks[0].status).toBe("done"));
    await user.click(within(article).getByRole("button", { name: `Completar ${goal.title}` }));
    await waitFor(() => expect(backend.goals[0].status).toBe("completed"));

    await user.click(screen.getByRole("tab", { name: "Historial" }));

    const sections = await screen.findAllByRole("region");
    expect(sections.map((section) => section.getAttribute("aria-label"))).toEqual([
      "Hoy",
      "Ayer",
      "Lunes, 10 de marzo de 2025",
    ]);

    const today = within(sections[0]);
    const rows = today.getAllByRole("listitem");
    expect(rows[0]).toHaveAccessibleName(`Meta completada: ${goal.title}`);
    expect(rows[1]).toHaveTextContent("Tarea");
    expect(rows[1]).toHaveTextContent(task.title);
    expect(within(rows[1]).getByTitle(goal.title)).toBeInTheDocument();

    const pr = within(sections[1]).getByRole("button", { name: /Nuevo.*Exportar facturas en PDF/ });
    expect(pr).toHaveTextContent("#12");
    expect(screen.queryByText("a1b2c3d")).not.toBeInTheDocument();
    await user.click(pr);
    expect(pr).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("a1b2c3d")).toBeInTheDocument();
    expect(screen.getByText("Genera el PDF con Typst.")).toBeInTheDocument();
    expect(
      within(screen.getByRole("list", { name: "Ficheros" })).getAllByRole("listitem"),
    ).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "PR #12" }));
    expect(backend.callsTo("open_url").at(-1)?.args).toEqual({
      url: "https://github.com/acme/facturas/pull/12",
    });

    const older = within(sections[2]);
    // The trailing time depends on the machine's time zone.
    const olderRows = older
      .getAllByRole("button")
      .map((row) => row.textContent?.replace(/\d\d:\d\d$/, ""));
    expect(olderRows).toEqual(["v0.1.0", "ArregloRedondear los totales"]);
    await user.click(older.getByRole("button", { name: "Versión v0.1.0" }));
    expect(backend.callsTo("open_url").at(-1)?.args).toEqual({
      url: "https://github.com/acme/facturas/releases/tag/v0.1.0",
    });

    await user.click(screen.getByRole("button", { name: "Código" }));
    expect(screen.queryByRole("region", { name: "Hoy" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Ayer" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Plan" }));
    expect(screen.getByRole("region", { name: "Hoy" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Ayer" })).not.toBeInTheDocument();
  });

  it("abre en Historial si es la pestaña inicial y añade la decisión que guarda un agente", async () => {
    localStorage.setItem("saispace.default-tab", "history");
    const { backend } = await renderApp({ projects: [project], gitHistory });

    expect(await screen.findByRole("tab", { name: "Historial" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await screen.findByRole("region", { name: "Ayer" });
    expect(screen.queryByRole("region", { name: "Hoy" })).not.toBeInTheDocument();

    await backend.agentSavesMemory({
      project_id: project.id,
      kind: "decision",
      title: "Generar los PDF con Typst",
    });

    const today = await screen.findByRole("region", { name: "Hoy" });
    expect(today).toHaveTextContent("Decisión");
    expect(today).toHaveTextContent("Generar los PDF con Typst");
  });
});
