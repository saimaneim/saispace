import { describe, expect, it } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import { makeProject } from "./fakeBackend";
import { renderApp } from "./renderApp";

const tree = {
  "/home/test": ["Dev", "Docs"],
  "/home/test/Dev": ["alpha", "beta"],
  "/home/test/Dev/alpha": [],
  "/home/test/Dev/beta": [],
  "/home/test/Docs": [],
};

const sidebar = () => within(screen.getByRole("navigation", { name: "Proyectos" }));

describe("flujo: crear proyecto", () => {
  it("desde vacío: abre el selector, entra en una carpeta, la añade y queda seleccionada", async () => {
    const { backend, user } = await renderApp({ directories: tree });

    expect(screen.getByRole("heading", { name: "Sin proyectos" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Añadir carpeta" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("option", { name: /Dev/ });

    await user.dblClick(within(dialog).getByRole("option", { name: /Dev/ }));
    await within(dialog).findByRole("option", { name: /alpha/ });
    await user.click(within(dialog).getByRole("button", { name: "Añadir alpha" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    expect(backend.callsTo("create_project").map((c) => c.args)).toEqual([
      { input: { name: "alpha", path: "/home/test/Dev/alpha" } },
    ]);
    expect(backend.projects).toHaveLength(1);

    // Persisted and re-read: the sidebar and the header come from list_projects.
    expect(await sidebar().findByText("alpha")).toBeInTheDocument();
    expect(sidebar().getByText("alpha").closest("[aria-current]")).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.queryByRole("heading", { name: "Sin proyectos" })).not.toBeInTheDocument();
    expect(screen.getByRole("tablist", { name: "Secciones del proyecto" })).toBeInTheDocument();
  });

  it("con teclado: filtra, Enter entra en la carpeta y Ctrl+Enter añade la actual", async () => {
    const { backend, user } = await renderApp({ directories: tree });

    await user.click(screen.getByRole("button", { name: "Añadir carpeta" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("option", { name: /Dev/ });

    // Enter with focus outside the filter input opens the selected folder.
    (document.activeElement as HTMLElement | null)?.blur();
    await user.keyboard("{Enter}");
    await within(dialog).findByRole("option", { name: /alpha/ });

    await user.type(within(dialog).getByRole("textbox", { name: "Filtrar carpetas" }), "bet");
    expect(within(dialog).queryByRole("option", { name: /alpha/ })).not.toBeInTheDocument();
    expect(within(dialog).getByRole("option", { name: /beta/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await user.keyboard("{Control>}{Enter}{/Control}");

    await waitFor(() =>
      expect(backend.callsTo("create_project").at(-1)?.args).toEqual({
        input: { name: "beta", path: "/home/test/Dev/beta" },
      }),
    );
    expect(await sidebar().findByText("beta")).toBeInTheDocument();
  });

  it("si la carpeta ya es un proyecto, lo selecciona y no crea otro", async () => {
    const existing = makeProject({ name: "alpha", path: "/home/test/Dev/alpha", sort_order: 1 });
    const other = makeProject({ name: "otro", path: "/home/test/otro", sort_order: 0 });
    const { backend, user } = await renderApp({ directories: tree, projects: [other, existing] });

    await sidebar().findByText("otro");
    await user.click(screen.getByRole("button", { name: "Añadir carpeta" }));
    const dialog = await screen.findByRole("dialog");
    await user.dblClick(await within(dialog).findByRole("option", { name: /Dev/ }));
    await user.click(await within(dialog).findByRole("button", { name: "Añadir alpha" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(backend.callsTo("create_project")).toHaveLength(0);
    expect(backend.projects).toHaveLength(2);
    await waitFor(() =>
      expect(sidebar().getByText("alpha").closest("[aria-current]")).toHaveAttribute(
        "aria-current",
        "page",
      ),
    );
    expect(screen.getByRole("heading", { level: 1, name: "alpha" })).toBeInTheDocument();
  });

  it("si el backend rechaza la creación, muestra el error y no añade nada", async () => {
    const { backend, user } = await renderApp({ directories: tree });
    backend.failNext("create_project", "No se pudo leer la carpeta");

    await user.click(screen.getByRole("button", { name: "Añadir carpeta" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Añadir Docs" }));

    const alert = await screen.findByText("No se pudo leer la carpeta");
    expect(alert.closest("[role=alert]")).toBeInTheDocument();
    expect(backend.projects).toHaveLength(0);
    expect(screen.getByRole("heading", { name: "Sin proyectos" })).toBeInTheDocument();
  });
});
