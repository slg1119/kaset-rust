import { commands } from "./bindings";

/** Use a solid sidebar if the native window effect cannot be read promptly. */
export async function detectWindowVibrancy(): Promise<boolean> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<false>((resolve) => {
    timer = setTimeout(() => resolve(false), 1000);
  });
  try {
    return await Promise.race([commands.windowVibrancy(), timeout]);
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}
