export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as unknown as Record<string, unknown>;
  return Boolean((w as any).__TAURI__ || (w as any).__TAURI_INTERNALS__);
}

export type UnlistenFn = () => void;

export async function tauriInvoke<T = unknown>(cmd: string, args?: unknown): Promise<T> {
  const mod = await import("@tauri-apps/api/core");
  return mod.invoke<T>(cmd, args as any);
}

export async function tauriListen<T = unknown>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<UnlistenFn> {
  const mod = await import("@tauri-apps/api/event");
  const unlisten = await mod.listen<T>(event, (e) => handler({ payload: e.payload }));
  return () => {
    try {
      unlisten();
    } catch {
      // ignore
    }
  };
}

