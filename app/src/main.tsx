import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { createQueryClient } from "./query";
import { detectWindowVibrancy } from "./vibrancy";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

// The sidebar is see-through only when Rust applied native vibrancy (macOS)
// or Mica (Windows 11); otherwise it stays a solid color. Ask before the
// first paint so the sidebar does not change after it appears.
const vibrancy = await detectWindowVibrancy();
document.documentElement.dataset.vibrancy = vibrancy ? "native" : "none";

createRoot(root).render(
  <StrictMode>
    <App queryClient={createQueryClient()} />
  </StrictMode>,
);
