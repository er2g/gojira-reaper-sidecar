import { isTauriRuntime } from "./tauri";

export interface ChatStore {
  get<T>(key: string): Promise<T | null>;
  set<T>(key: string, value: T): Promise<void>;
  delete(key: string): Promise<void>;
  save(): Promise<void>;
}

class LocalStorageChatStore implements ChatStore {
  private prefix = "chat:";

  async get<T>(key: string): Promise<T | null> {
    const raw = window.localStorage.getItem(this.prefix + key);
    if (!raw) return null;
    try {
      return JSON.parse(raw) as T;
    } catch {
      return null;
    }
  }

  async set<T>(key: string, value: T): Promise<void> {
    window.localStorage.setItem(this.prefix + key, JSON.stringify(value));
  }

  async delete(key: string): Promise<void> {
    window.localStorage.removeItem(this.prefix + key);
  }

  async save(): Promise<void> {
    // no-op for localStorage
  }
}

let cached: ChatStore | null = null;

export async function getChatStore(): Promise<ChatStore> {
  if (cached) return cached;
  if (!isTauriRuntime()) {
    cached = new LocalStorageChatStore();
    return cached;
  }

  const mod = await import("@tauri-apps/plugin-store");
  // Stored under AppData (tauri-plugin-store default). Separate file from prefs.
  cached = new mod.Store("chats.bin") as unknown as ChatStore;
  return cached;
}

