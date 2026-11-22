import { render } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useShortcuts } from "./useShortcuts";

function Harness({ back }: { back: () => void }) {
  useShortcuts({ back });
  return null;
}

test("skips a shortcut whose key a component already handled", () => {
  const back = vi.fn();
  render(<Harness back={back} />);
  // The open queue panel handles Escape in the capture phase and prevents the default.
  const handled = (event: KeyboardEvent) => event.preventDefault();
  window.addEventListener("keydown", handled, true);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", cancelable: true }));
  window.removeEventListener("keydown", handled, true);
  expect(back).not.toHaveBeenCalled();

  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", cancelable: true }));
  expect(back).toHaveBeenCalledTimes(1);
});
