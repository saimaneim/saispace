import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MemoryLinks } from "../components/MemoryLinks";
import { installFakeBackend, makeMemory } from "./fakeBackend";

describe("MemoryLinks · disparadores", () => {
  it("muestra los disparadores y traduce env:<host> a su nombre", async () => {
    const memory = makeMemory({ id: "m1", title: "Clave SSH en T3 Code" });
    const long = "permission denied (publickey) ".repeat(6).trim();
    installFakeBackend({
      memories: [memory],
      triggers: { m1: ["env:t3code", long] },
    });
    render(<MemoryLinks memory={memory} onOpen={() => {}} />);

    expect(await screen.findByText("Vuelve sola cuando aparece · 2")).toBeInTheDocument();
    expect(screen.getByText("Entorno: T3 Code")).toBeInTheDocument();
    const chip = screen.getByText(long);
    expect(chip.className).toContain("break-all");
    expect(chip.className).toContain("max-w-full");
  });

  it("no muestra nada sin enlaces ni disparadores", async () => {
    const memory = makeMemory({ id: "m2" });
    installFakeBackend({ memories: [memory] });
    const { container } = render(<MemoryLinks memory={memory} onOpen={() => {}} />);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(container).toBeEmptyDOMElement();
  });
});
