import { describe, expect, it } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import { makeMemory, makeProject } from "./fakeBackend";
import { renderApp } from "./renderApp";

const project = makeProject({ id: "proj-saispace", name: "saispace", path: "/home/test/saispace" });

async function openMemoryTab(user: Awaited<ReturnType<typeof renderApp>>["user"]) {
  await user.click(await screen.findByRole("tab", { name: /Memoria/ }));
}

describe("flujo: guardar y leer memoria", () => {
  it("lee las memorias del proyecto y muestra el detalle de la seleccionada", async () => {
    const { backend, user } = await renderApp({
      projects: [project],
      memories: [
        makeMemory({
          project_id: project.id,
          project_name: project.name,
          kind: "decision",
          title: "Usar SQLite en WAL",
          content: "Varios procesos leen a la vez.",
          topic_key: "arch/db",
        }),
        makeMemory({ project_id: "otro", title: "De otro proyecto" }),
        makeMemory({ project_id: null, title: "Global, vive en la vista Global" }),
      ],
    });

    await openMemoryTab(user);

    expect(await screen.findByRole("heading", { name: "Usar SQLite en WAL" })).toBeInTheDocument();
    expect(screen.getByText("Varios procesos leen a la vez.")).toBeInTheDocument();
    expect(screen.getAllByText("Decisión").length).toBeGreaterThan(0);
    expect(screen.queryByText("De otro proyecto")).not.toBeInTheDocument();
    expect(screen.queryByText("Global, vive en la vista Global")).not.toBeInTheDocument();
    expect(backend.callsTo("list_memories").at(-1)?.args).toMatchObject({
      projectId: project.id,
    });
  });

  it("filtra memorias por texto y abre la seleccionada", async () => {
    const m1 = makeMemory({
      project_id: project.id,
      title: "Configuración SQLite",
      content: "Modo WAL activado.",
      topic_key: "arch/db",
      // Newest first in the list: it must be the one selected on open.
      updated_at: "2026-11-27T10:00:00.000Z",
    });
    const m2 = makeMemory({
      project_id: project.id,
      title: "Convención de nombres",
      content: "Usar snake_case para variables.",
      topic_key: "style/naming",
      updated_at: "2026-11-26T10:00:00.000Z",
    });

    const { user } = await renderApp({
      projects: [project],
      memories: [m1, m2],
    });

    await openMemoryTab(user);
    expect(
      await screen.findByRole("heading", { name: "Configuración SQLite" }),
    ).toBeInTheDocument();

    const searchInput = screen.getByRole("textbox", { name: "Filtrar memorias" });
    await user.type(searchInput, "nombres");

    expect(
      await screen.findByRole("button", { name: /Convención de nombres/ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Configuración SQLite/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Convención de nombres/ }));
    expect(
      await screen.findByRole("heading", { name: "Convención de nombres" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Usar snake_case para variables.")).toBeInTheDocument();
  });

  it("una memoria guardada por un agente aparece sin recargar (evento saispace:activity)", async () => {
    const { backend, user } = await renderApp({ projects: [project] });
    await openMemoryTab(user);
    expect(await screen.findByText(/save_memory/)).toBeInTheDocument();

    await backend.agentSavesMemory({
      project_id: project.id,
      project_name: project.name,
      kind: "bugfix",
      title: "El watcher pierde eventos",
      content: "Causa: debounce de 0 ms.",
      topic_key: "bug/watcher",
    });

    expect(
      await screen.findByRole("heading", { name: "El watcher pierde eventos" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Causa: debounce de 0 ms.")).toBeInTheDocument();

    // Same topic_key updates in place instead of duplicating.
    await backend.agentSavesMemory({
      project_id: project.id,
      topic_key: "bug/watcher",
      title: "El watcher pierde eventos (resuelto)",
    });
    expect(
      await screen.findByRole("heading", { name: "El watcher pierde eventos (resuelto)" }),
    ).toBeInTheDocument();
    expect(backend.memories).toHaveLength(1);
  });

  it("edita una memoria, la guarda y relee el valor persistido", async () => {
    const memory = makeMemory({
      project_id: project.id,
      project_name: project.name,
      title: "Borrador",
      content: "texto viejo",
      topic_key: "notas/a",
    });
    const { backend, user } = await renderApp({ projects: [project], memories: [memory] });
    await openMemoryTab(user);
    await screen.findByRole("heading", { name: "Borrador" });

    await user.click(screen.getByRole("button", { name: "Editar" }));
    const dialog = await screen.findByRole("dialog", { name: "Editar memoria" });
    const title = within(dialog).getByRole("textbox", { name: "Título" });
    const topic = within(dialog).getByRole("textbox", { name: "Tema" });
    const content = within(dialog).getByRole("textbox", { name: "Contenido" });
    expect(title).toHaveValue("Borrador");

    await user.clear(title);
    await user.type(title, "  Decisión final  ");
    await user.clear(topic);
    await user.clear(content);
    await user.type(content, "texto nuevo");
    await user.click(within(dialog).getByRole("button", { name: "Guardar" }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Editar memoria" })).not.toBeInTheDocument(),
    );
    expect(backend.callsTo("update_memory").map((c) => c.args)).toEqual([
      {
        id: memory.id,
        input: { title: "Decisión final", kind: "note", topic_key: "", content: "texto nuevo" },
      },
    ]);
    // Stored value, then the UI shows what list_memories returns, not local state.
    expect(backend.memories[0]).toMatchObject({
      title: "Decisión final",
      content: "texto nuevo",
      topic_key: null,
    });
    expect(await screen.findByRole("heading", { name: "Decisión final" })).toBeInTheDocument();
    expect(screen.getByText("texto nuevo")).toBeInTheDocument();
  });

  it("si guardar falla, el diálogo sigue abierto con el error y no cambia nada", async () => {
    const memory = makeMemory({ project_id: project.id, title: "Intacta", content: "x" });
    const { backend, user } = await renderApp({ projects: [project], memories: [memory] });
    await openMemoryTab(user);
    await screen.findByRole("heading", { name: "Intacta" });
    backend.failNext("update_memory", "database is locked", "database");

    await user.click(screen.getByRole("button", { name: "Editar" }));
    const dialog = await screen.findByRole("dialog", { name: "Editar memoria" });
    await user.type(within(dialog).getByRole("textbox", { name: "Título" }), " 2");
    await user.click(within(dialog).getByRole("button", { name: "Guardar" }));

    expect(await within(dialog).findByText("database is locked")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Editar memoria" })).toBeInTheDocument();
    expect(backend.memories[0].title).toBe("Intacta");
  });

  it("fijar y eliminar persisten y la lista se relee", async () => {
    const a = makeMemory({
      project_id: project.id,
      title: "Alfa",
      updated_at: "2026-01-02T00:00:00Z",
    });
    const b = makeMemory({
      project_id: project.id,
      title: "Beta",
      updated_at: "2026-01-01T00:00:00Z",
    });
    const { backend, user } = await renderApp({ projects: [project], memories: [a, b] });
    await openMemoryTab(user);
    await screen.findByRole("heading", { name: "Alfa" });

    await user.click(screen.getByRole("button", { name: /Beta/ }));
    await screen.findByRole("heading", { name: "Beta" });
    await user.click(screen.getByRole("button", { name: "Fijar" }));
    await screen.findByRole("button", { name: "Desfijar" });
    expect(backend.memories.find((m) => m.id === b.id)?.pinned).toBe(true);

    await user.click(screen.getByRole("button", { name: "Eliminar" }));
    const confirm = await screen.findByRole("dialog", { name: "Eliminar memoria" });
    await user.click(within(confirm).getByRole("button", { name: "Eliminar" }));

    await waitFor(() => expect(backend.memories.map((m) => m.title)).toEqual(["Alfa"]));
    expect(await screen.findByRole("heading", { name: "Alfa" })).toBeInTheDocument();
    expect(screen.queryByText("Beta")).not.toBeInTheDocument();
  });
});

describe("flujo: abrir una memoria global desde el buscador", () => {
  it("en la vista Global, el resultado abre su detalle", async () => {
    const global = makeMemory({
      project_id: null,
      title: "Webs con sesión",
      content: "Usa su navegador.",
    });
    const { user } = await renderApp({
      projects: [project],
      memories: [makeMemory({ project_id: null, title: "Otra global" }), global],
    });

    await user.click(screen.getByRole("button", { name: /Global/ }));
    await user.keyboard("{Control>}k{/Control}");
    await user.type(await screen.findByLabelText("Consulta de búsqueda"), "webs");
    await user.click(await screen.findByRole("button", { name: /Webs con sesión/ }));

    expect(await screen.findByRole("heading", { name: "Webs con sesión" })).toBeInTheDocument();
    expect(screen.getByText("Usa su navegador.")).toBeInTheDocument();
  });
});
