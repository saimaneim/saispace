import { describe, expect, it } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import { makeGoal, makeProject, makeTask } from "./fakeBackend";
import { renderApp } from "./renderApp";

const project = makeProject({ id: "proj-plan", name: "saispace", path: "/home/test/saispace" });

describe("flujo: Plan", () => {
  it("crea una meta con criterio y la desglosa en tareas", async () => {
    const { backend, user } = await renderApp({ projects: [project] });
    const title = "Lanzar la beta";

    await user.type(await screen.findByRole("textbox", { name: "Nueva meta" }), title);
    await user.click(screen.getByRole("button", { name: "Añadir criterio de finalización" }));
    await user.type(
      screen.getByRole("textbox", { name: "Criterio de finalización" }),
      "El instalador pasa las pruebas",
    );
    await user.keyboard("{Enter}");

    const article = await screen.findByRole("article", { name: title });
    expect(within(article).getByText(/Hecho cuando:/)).toHaveTextContent(
      "Hecho cuando: El instalador pasa las pruebas",
    );
    expect(backend.callsTo("goal_create").at(-1)?.args).toEqual({
      projectId: project.id,
      title,
      criteria: "El instalador pasa las pruebas",
      tasks: [],
    });

    const goalId = backend.goals[0].id;
    for (const taskTitle of ["Preparar el instalador", "Probar una instalación limpia"]) {
      const input = within(article).getByRole("textbox", {
        name: `Nueva tarea para ${title}`,
      });
      await user.type(input, taskTitle);
      await user.keyboard("{Enter}");
      expect(await within(article).findByRole("checkbox", { name: taskTitle })).toBeInTheDocument();
    }

    expect(within(article).getByText("0/2")).toBeInTheDocument();
    expect(backend.callsTo("create_task").map((call) => call.args)).toEqual([
      { input: { project_id: project.id, title: "Preparar el instalador", goal_id: goalId } },
      {
        input: { project_id: project.id, title: "Probar una instalación limpia", goal_id: goalId },
      },
    ]);
    expect(backend.tasks).toHaveLength(2);
  });

  it("marca una tarea de forma optimista y actualiza el contador", async () => {
    const goal = makeGoal({ id: "goal-launch", project_id: project.id, title: "Lanzamiento" });
    const done = makeTask({
      id: "task-done",
      project_id: project.id,
      goal_id: goal.id,
      title: "Diseño terminado",
      status: "done",
    });
    const todo = makeTask({
      id: "task-todo",
      project_id: project.id,
      goal_id: goal.id,
      title: "Publicar release",
    });
    const { backend, user } = await renderApp({
      projects: [project],
      goals: [goal],
      tasks: [done, todo],
    });

    const article = await screen.findByRole("article", { name: goal.title });
    expect(within(article).getByText("1/2")).toBeInTheDocument();
    await user.click(within(article).getByRole("checkbox", { name: todo.title }));

    expect(within(article).getByText("2/2")).toBeInTheDocument();
    expect(within(article).getByRole("checkbox", { name: todo.title })).toBeChecked();
    expect(backend.tasks.find((task) => task.id === todo.id)?.status).toBe("done");
    expect(within(article).getByText("Todos los pasos están listos.")).toBeInTheDocument();
    expect(backend.callsTo("update_task").at(-1)?.args).toEqual({
      id: todo.id,
      input: { status: "done" },
    });
  });

  it("revierte el checkbox y el contador si el backend falla", async () => {
    const goal = makeGoal({ id: "goal-rollback", project_id: project.id, title: "Meta estable" });
    const done = makeTask({
      id: "task-stable-done",
      project_id: project.id,
      goal_id: goal.id,
      title: "Primer paso",
      status: "done",
    });
    const todo = makeTask({
      id: "task-stable-todo",
      project_id: project.id,
      goal_id: goal.id,
      title: "Segundo paso",
    });
    const { backend, user } = await renderApp({
      projects: [project],
      goals: [goal],
      tasks: [done, todo],
    });
    const article = await screen.findByRole("article", { name: goal.title });
    backend.failNext("update_task", "No se pudo actualizar la tarea");

    await user.click(within(article).getByRole("checkbox", { name: todo.title }));

    expect(await screen.findByRole("alert")).toHaveTextContent("No se pudo actualizar la tarea");
    await waitFor(() => expect(within(article).getByText("1/2")).toBeInTheDocument());
    expect(within(article).getByRole("checkbox", { name: todo.title })).not.toBeChecked();
    expect(backend.tasks.find((task) => task.id === todo.id)?.status).toBe("todo");
  });

  it("agrupa por hitos: activos arriba, completados plegados al final y archivados fuera", async () => {
    const active = makeGoal({
      id: "goal-active",
      project_id: project.id,
      title: "Meta activa",
      position: 0,
    });
    const completed = makeGoal({
      id: "goal-completed",
      project_id: project.id,
      title: "Meta completada",
      status: "completed",
      position: 1,
      completed_at: new Date().toISOString(),
    });
    const archived = makeGoal({
      id: "goal-archived",
      project_id: project.id,
      title: "Meta archivada",
      status: "archived",
      position: 2,
    });
    const { user } = await renderApp({ projects: [project], goals: [active, completed, archived] });

    expect(await screen.findByRole("article", { name: active.title })).toBeInTheDocument();
    expect(screen.queryByRole("article", { name: completed.title })).not.toBeInTheDocument();
    expect(screen.queryByRole("article", { name: archived.title })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Completadas: 1" }));
    expect(await screen.findByRole("article", { name: completed.title })).toBeInTheDocument();
    expect(screen.queryByRole("article", { name: archived.title })).not.toBeInTheDocument();
  });

  it("muestra Sin meta solo cuando hay tareas sueltas y permite plegarlas", async () => {
    const goal = makeGoal({ id: "goal-with-tasks", project_id: project.id, title: "Con meta" });
    const assigned = makeTask({
      id: "task-assigned",
      project_id: project.id,
      goal_id: goal.id,
      title: "Asignada",
    });
    const loose = makeTask({
      id: "task-loose",
      project_id: project.id,
      title: "Suelta",
    });
    const { user } = await renderApp({
      projects: [project],
      goals: [goal],
      tasks: [assigned, loose],
    });

    const toggle = await screen.findByRole("button", { name: /Sin meta/ });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("checkbox", { name: loose.title })).not.toBeInTheDocument();
    await user.click(toggle);
    expect(await screen.findByRole("checkbox", { name: loose.title })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: assigned.title })).toBeInTheDocument();
  });

  it("oculta por completo Sin meta cuando no hay tareas sueltas", async () => {
    const goal = makeGoal({ id: "goal-no-loose", project_id: project.id, title: "Todo asignado" });
    await renderApp({ projects: [project], goals: [goal] });

    expect(await screen.findByRole("article", { name: goal.title })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Sin meta/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Tareas sin meta" })).not.toBeInTheDocument();
  });

  it("ordena las pestañas como Contexto, Plan, Historial, Tecnologías y Memoria y conserva la actividad", async () => {
    const { user } = await renderApp({ projects: [project] });
    await screen.findByRole("textbox", { name: "Nueva meta" });

    const tabs = within(
      screen.getByRole("tablist", { name: "Secciones del proyecto" }),
    ).getAllByRole("tab");
    expect(tabs.map((tab) => tab.querySelector("span")?.textContent)).toEqual([
      "Contexto",
      "Plan",
      "Historial",
      "Tecnologías",
      "Memoria",
    ]);
    expect(screen.getByRole("tab", { name: /^Plan/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("tab", { name: /Tareas/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Mostrar actividad" }));
    expect(await screen.findByRole("region", { name: "Actividad" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ocultar actividad" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
});
