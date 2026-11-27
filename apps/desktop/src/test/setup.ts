import { afterEach, expect } from "bun:test";
import { clearMocks } from "@tauri-apps/api/mocks";
import type { TestingLibraryMatchers } from "@testing-library/jest-dom/matchers";
import * as matchers from "@testing-library/jest-dom/matchers";
import { cleanup } from "@testing-library/react";

declare module "bun:test" {
  interface Matchers<T = unknown> extends TestingLibraryMatchers<unknown, T> {}
}

expect.extend(matchers);

if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.releasePointerCapture) {
  Element.prototype.releasePointerCapture = () => {};
}

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
  delete (globalThis as { isTauri?: boolean }).isTauri;
  localStorage.clear();
});
