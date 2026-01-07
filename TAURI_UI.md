# TAURI_UI.md (v2.0 - Production Ready)

## 1. PROJECT IDENTITY

Role: Senior Frontend Architect & UX Designer.
Objective: Build a commercial-grade, secure, and reactive Desktop UI using Tauri v2 (Rust) + React.

Core Philosophy:

- Actor Model Networking: The UI never touches the raw WebSocket. It communicates via channels to a dedicated Rust background task.
- Zero-Trust Security: API Keys are stored in an encrypted vault (Stronghold), not plain text files.
- Determinstic AI: Enforce JSON Schemas on Gemini to guarantee valid parameter generation.

## 2. TECH STACK & DEPENDENCIES

Tauri Plugins:

- tauri-plugin-stronghold: For encrypted API Key storage.
- tauri-plugin-single-instance: Prevent multiple UI windows (conflicts with the DLL).
- tauri-plugin-shell: For opening links (optional).
- tauri-plugin-store: ONLY for non-sensitive UI prefs (last theme, last target GUID).

Rust Crates:

- tokio: For async runtime.
- tokio-tungstenite: For async WebSocket client (more robust than ws).
- reqwest: With json and rustls-tls features.
- keyring (Optional alternative, but Stronghold is preferred for Tauri v2).

## 3. ARCHITECTURE: The "Actor Model" Bridge

Problem: Wrapping a WebSocket writer in a Mutex causes deadlocks and blocks the UI thread.
Solution: A dedicated WebSocketTask that owns the socket.

### A. AppState (The Handle)

Rust struct

```rust
struct AppState {
  // Send commands to the background WebSocket Task
  tx: mpsc::Sender<UiCommand>,
  // Cache for generating "Diffs" (Last applied params per GUID)
  param_cache: Mutex<HashMap<String, Vec<ParamChange>>>,
}

enum UiCommand {
  Connect,
  SendToDll(ClientCommand),
  Disconnect,
}
```

### B. The WebSocket Actor (Background Task)

A tokio::spawn loop that:

- Manages the tokio_tungstenite connection.
- Reconnect Loop: Implements exponential backoff (1s, 2s, 5s...) if connection is lost.
- State Machine: Emits events to Frontend via app_handle.emit():
  - reaper://status -> { status: "connecting" | "connected" | "disconnected", retry_in: 0 }
  - reaper://handshake -> { instances: [...] }
  - reaper://project_changed -> null

## 4. SECURITY: API Key Storage

NEVER use tauri-plugin-store for the API Key.

Implementation: Use Tauri Stronghold.

Flow:

1. User enters Key in React.
2. Rust: save_api_key(key) -> Encrypts and saves to Stronghold vault.
3. Rust: generate_tone(...) -> Auto-loads key from vault (never sent to UI).

## 5. GEMINI INTEGRATION (Structured Output)

To prevent hallucinated JSON, we use Gemini's structured output schema.

NOTE (REST vs SDK): When calling the REST API directly (e.g. Rust `reqwest`), the `generationConfig` fields are camelCase.

Payload Construction:

JSON

```json
{
  "contents": [{ "parts": [{ "text": "..." }] }],
  "generationConfig": {
    "responseMimeType": "application/json",
    "responseJsonSchema": {
      "type": "OBJECT",
      "properties": {
        "reasoning": { "type": "STRING" },
        "params": {
          "type": "ARRAY",
          "items": {
            "type": "OBJECT",
            "properties": {
              "index": { "type": "INTEGER" },
              "value": { "type": "NUMBER" }
            },
            "required": ["index", "value"]
          }
        }
      },
      "required": ["reasoning", "params"]
    }
  }
}
```

Retry Policy:

- Implement a simple 3-try loop with backoff for 429 Too Many Requests or 5xx errors.

## 6. FRONTEND UX & FEATURES (React)

### A. Connection Status Bar (Top)

Visuals:

- Gray Dot: "Disconnected (Retrying in 3s...)"
- Yellow Dot: "Connecting..."
- Green Dot: "Connected to Reaper"

Action:

- "Reconnect Now" button (forces UiCommand::Connect).

### B. The "Engineer's Notebook" (Reasoning Display)

Label: "Engineer's Notes" (Avoid "AI Thoughts" to manage expectations).

Diff View:

When a tone is generated, compare it against AppState::param_cache.

Display changes:

- Gain: 0.4 ➔ 0.6
- Delay: Bypassed

Visual:

- Green for additions
- Red for removals/reductions

### C. "Dry Run" Mode (Preview)

Toggle:

- "Preview Only" checkbox.

Logic:

- Call Gemini.
- Show Reasoning + Parameter List/Diff in UI.
- Do NOT send to WebSocket.
- Show "Apply" button to confirm and send.

## 7. EXECUTION STEPS FOR AGENT

Dependencies:

- Add tokio, tokio-tungstenite, tauri-plugin-stronghold, tauri-plugin-single-instance.

Rust Core:

- Create ws_actor.rs: The async task handling connection/reconnection/writing.
- Create app_state.rs: The mpsc channel holder.
- Implement Stronghold setup in main.rs.

Gemini Service:

- Implement `gemini.rs` with a `responseMimeType` + `responseJsonSchema` payload (REST, camelCase).

Tauri Commands:

- connect_ws() -> Sends UiCommand::Connect.
- generate_tone(preview: bool) -> Calls Gemini -> (If !preview) Sends UiCommand::SendToDll.
- get_diff(new_params) -> Compares with cache.

Frontend:

- Build StatusBadge component listening to reaper://status.
- Build DiffViewer component.
- Wire "Generate" button to generate_tone.

CRITICAL REMINDER:

- Ensure tauri.conf.json enables the necessary permissions for stronghold, http (for Gemini), and single-instance.

GO! Build the Brain.
