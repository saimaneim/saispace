import { describe, expect, it } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import { makeProject, makeTechnology } from "./fakeBackend";
import { renderApp } from "./renderApp";

const project = makeProject({ id: "proj-tech", name: "saispace", path: "/home/test/saispace" });

async function openTechTab(user: Awaited<ReturnType<typeof renderApp>>["user"]) {
  await user.click(await screen.findByRole("tab", { name: /^Tecnologías/ }));
}

describe("flujo: Tecnologías", () => {
  it("no muestra botón Escanear y sincroniza automáticamente el stack al entrar", async () => {
    const react = makeTechnology({
      project_id: project.id,
      name: "react",
      version: "19.3.1",
      ecosystem: "npm",
    });
    const tokio = makeTechnology({
      project_id: project.id,
      name: "tokio",
      version: "1.43.0",
      ecosystem: "cargo",
    });

    const { backend, user } = await renderApp({
      projects: [project],
      technologies: [react, tokio],
    });

    await openTechTab(user);

    expect(screen.queryByRole("button", { name: /escanear/i })).not.toBeInTheDocument();

    await waitFor(() => expect(backend.callsTo("sync_project_stack").length).toBeGreaterThan(0));

    expect(await screen.findByText("react")).toBeInTheDocument();
    expect(screen.getByText("19.3.1")).toBeInTheDocument();
    expect(screen.getByText("tokio")).toBeInTheDocument();
    expect(screen.getByText("1.43.0")).toBeInTheDocument();
    expect(screen.getByText(/JavaScript y TypeScript/)).toBeInTheDocument();
    expect(screen.getByText(/Rust/)).toBeInTheDocument();
  });
});
