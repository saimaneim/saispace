import { expect } from "bun:test";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "../App";
import { type FakeBackendOptions, installFakeBackend } from "./fakeBackend";

export async function renderApp(options: FakeBackendOptions = {}) {
  const backend = installFakeBackend(options);
  const user = userEvent.setup();
  const utils = render(<App />);
  await screen.findByRole("button", { name: "Añadir carpeta" });
  await waitFor(() => expect(backend.callsTo("list_projects").length).toBeGreaterThan(0));
  return { backend, user, ...utils };
}
