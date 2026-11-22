import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { commands, events, mockCommands } from "../bindings";
import { useAuthStatus } from "./useAuthStatus";

test("subscribes before asking for the current state", async () => {
  const order: string[] = [];
  mockCommands({
    authStatus: () => {
      order.push("authStatus");
      return "signed_out";
    },
  });
  const listen = events.authChanged.listen;
  vi.spyOn(events.authChanged, "listen").mockImplementation(async (callback) => {
    const stop = await listen(callback);
    order.push("listening");
    return stop;
  });
  const { result } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(result.current.status).toBe("signed_out"));
  expect(order).toEqual(["listening", "authStatus"]);
});

test("an event before the reply wins", async () => {
  let reply: ((status: "signed_out") => void) | undefined;
  mockCommands({ authStatus: () => new Promise((resolve) => (reply = resolve)) });
  const { result } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(reply).toBeDefined());
  await act(() => events.authChanged.emit({ status: "signed_in" }));
  await act(async () => reply?.("signed_out"));
  expect(result.current.status).toBe("signed_in");
});

test("unmounting stops listening", async () => {
  const stop = vi.fn();
  vi.spyOn(events.authChanged, "listen").mockResolvedValue(stop);
  vi.spyOn(commands, "authStatus").mockResolvedValue("signed_out");
  const { result, unmount } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(result.current.status).toBe("signed_out"));
  unmount();
  expect(stop).toHaveBeenCalledTimes(1);
});

test("unmounting before the subscription resolves still unsubscribes", async () => {
  const stop = vi.fn();
  let resolveListen: (stop: () => void) => void = () => {};
  vi.spyOn(events.authChanged, "listen").mockReturnValue(
    new Promise((resolve) => (resolveListen = resolve)),
  );
  const authStatus = vi.spyOn(commands, "authStatus").mockResolvedValue("signed_out");
  const { unmount } = renderHook(() => useAuthStatus());
  unmount();
  await act(async () => resolveListen(stop));
  expect(stop).toHaveBeenCalledTimes(1);
  expect(authStatus).not.toHaveBeenCalled();
});

test("a failed read reports the error until retried", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => {
      calls += 1;
      if (calls === 1) throw { kind: "internal", message: "store unavailable" };
      return "signed_in";
    },
  });
  const { result } = renderHook(() => useAuthStatus());
  await waitFor(() => expect(result.current.error).toBe("store unavailable"));
  expect(result.current.status).toBe(null);
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.status).toBe("signed_in"));
  expect(result.current.error).toBe(null);
});
