import "@testing-library/jest-dom/vitest";
import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// happy-dom lacks a few layout APIs that Radix and the directory picker call.
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.releasePointerCapture = () => {};
}

afterEach(async () => {
  cleanup();
  // Unmount runs `unlisten()` from a resolved promise; let it reach the mocked
  // event plugin before its internals are torn down.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
  delete (globalThis as { isTauri?: boolean }).isTauri;
  localStorage.clear();
});
