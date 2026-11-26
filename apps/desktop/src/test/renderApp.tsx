import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, vi } from "vitest";
import { App } from "../App";
import { type FakeBackendOptions, installFakeBackend } from "./fakeBackend";

/**
 * Mount the whole desktop shell against the in-memory Tauri backend.
 * Returns the backend (to seed data and inspect calls) and a user-event
 * instance bound to the document.
 */
export async function renderApp(options: FakeBackendOptions = {}) {
  const backend = installFakeBackend(options);
  const user = userEvent.setup();
  const utils = render(<App />);
  // First paint is done once the sidebar's add button is in place and the
  // initial `list_projects` round-trip has settled.
  await screen.findByRole("button", { name: "Añadir carpeta" });
  await vi.waitFor(() => expect(backend.callsTo("list_projects").length).toBeGreaterThan(0));
  return { backend, user, ...utils };
}
