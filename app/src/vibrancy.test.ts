import { expect, test, vi } from "vitest";
import { commands, mockCommands } from "./bindings";
import { detectWindowVibrancy } from "./vibrancy";

test("a stalled vibrancy command falls back after one second", async () => {
  vi.useFakeTimers();
  try {
    mockCommands({ windowVibrancy: () => new Promise(() => {}) });
    const settled = vi.fn();
    void detectWindowVibrancy().then(settled);
    await vi.advanceTimersByTimeAsync(999);
    expect(settled).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(settled).toHaveBeenCalledExactlyOnceWith(false);
  } finally {
    vi.useRealTimers();
  }
});

test("vibrancy uses the native value and clears a finished request's timer", async () => {
  vi.useFakeTimers();
  try {
    mockCommands({ windowVibrancy: () => true });
    await expect(detectWindowVibrancy()).resolves.toBe(true);
    vi.spyOn(commands, "windowVibrancy").mockRejectedValue(new Error("IPC unavailable"));
    await expect(detectWindowVibrancy()).resolves.toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  } finally {
    vi.useRealTimers();
  }
});
