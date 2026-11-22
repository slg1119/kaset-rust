import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, vi } from "vitest";

afterEach(async () => {
  cleanup();
  // Let effect cleanups (event unlisten promises) settle while the IPC mock
  // still exists, then remove it.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
  // Spies on the generated `commands` / `events` objects must not leak into the next test.
  vi.restoreAllMocks();
});
