import { describe, expect, it } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import { makeContextFile, makeProject } from "./fakeBackend";
import { renderApp } from "./renderApp";

const project = makeProject({ id: "proj-ctx", name: "saispace", path: "/home/test/saispace" });

async function openContextTab(user: Awaited<ReturnType<typeof renderApp>>["user"]) {
  await user.click(await screen.findByRole("tab", { name: /^Contexto/ }));
}

describe("flujo: Contexto", () => {
  it("agrupa archivos en .agents/checks/*.toml bajo la categoría Checks", async () => {
    const checkFile = makeContextFile({
      path: ".agents/checks/security.toml",
      name: "security.toml",
      kind: "other",
    });
    const agentFile = makeContextFile({
      path: "AGENTS.md",
      name: "AGENTS.md",
      kind: "agents",
    });

    const { backend, user } = await renderApp({
      projects: [project],
      contextFiles: [agentFile, checkFile],
      contextContents: {
        "AGENTS.md": "# Guidelines",
        ".agents/checks/security.toml": "name = 'security'\ncommand = 'cargo audit'",
      },
    });

    await openContextTab(user);

    const checksButton = await screen.findByRole("button", { name: /Checks/ });
    expect(checksButton).toBeInTheDocument();
    expect(checksButton).toHaveTextContent("1");

    await user.click(checksButton);
    const fileButton = await screen.findByRole("button", { name: "security.toml" });
    expect(fileButton).toBeInTheDocument();

    await user.click(fileButton);
    await waitFor(() =>
      expect(backend.callsTo("read_context_file").at(-1)?.args).toMatchObject({
        relativePath: ".agents/checks/security.toml",
      }),
    );
  });

  it("abre y cierra carpetas de skills incluso con un archivo interno seleccionado", async () => {
    const skillDoc = makeContextFile({
      path: ".agents/skills/saispace/SKILL.md",
      name: "SKILL.md",
      kind: "other",
    });
    const skillRef = makeContextFile({
      path: ".agents/skills/saispace/reference.md",
      name: "reference.md",
      kind: "other",
    });

    const { user } = await renderApp({
      projects: [project],
      contextFiles: [skillDoc, skillRef],
      contextContents: {
        ".agents/skills/saispace/SKILL.md": "# Saispace Skill",
        ".agents/skills/saispace/reference.md": "# Reference Guide",
      },
    });

    await openContextTab(user);

    const skillFolder = await screen.findByRole("button", { name: /^saispace/ });
    expect(skillFolder).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "SKILL.md" })).toBeInTheDocument();

    await user.click(skillFolder);
    expect(skillFolder).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: "SKILL.md" })).not.toBeInTheDocument();

    await user.click(skillFolder);
    expect(skillFolder).toHaveAttribute("aria-expanded", "true");
    expect(await screen.findByRole("button", { name: "SKILL.md" })).toBeInTheDocument();
  });
});
