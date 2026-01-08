import { isTauriRuntime } from "./tauri";

export interface PrefsStore {
  get<T>(key: string): Promise<T | null>;
  set<T>(key: string, value: T): Promise<void>;
  delete(key: string): Promise<void>;
  save(): Promise<void>;
}

class LocalStoragePrefsStore implements PrefsStore {
  async get<T>(key: string): Promise<T | null> {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    try {
      return JSON.parse(raw) as T;
    } catch {
      return null;
    }
  }

  async set<T>(key: string, value: T): Promise<void> {
    window.localStorage.setItem(key, JSON.stringify(value));
  }

  async delete(key: string): Promise<void> {
    window.localStorage.removeItem(key);
  }

  async save(): Promise<void> {
    // no-op for localStorage
  }
}

let cached: PrefsStore | null = null;

export async function getPrefsStore(): Promise<PrefsStore> {
  if (cached) return cached;
  if (!isTauriRuntime()) {
    cached = new LocalStoragePrefsStore();
    return cached;
  }

  const mod = await import("@tauri-apps/plugin-store");
  cached = new mod.Store("prefs.bin") as unknown as PrefsStore;
  return cached;
}

