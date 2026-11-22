import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach } from "vitest";

afterEach(async () => {
  cleanup();
  // Let effect cleanups (event unlisten promises) settle while the IPC mock
  // still exists, then remove it.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});
