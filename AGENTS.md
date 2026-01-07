# AGENTS.md (v4.1 — Final Production Spec + Hardening Patch)

## 1. PROJECT IDENTITY

**Role:** Lead Systems Architect (Rust/Audio)
**Objective:** Commercial-grade, crash-proof Reaper integration for Neural DSP Gojira.
**Non-negotiables:**

* **STRICT THREAD SEPARATION:** Network thread **asla** Reaper API çağırmaz.
* **NO POINTER CACHING:** `MediaTrack*` / `Track*` pointer’ı **asla** frame’ler arasında saklanmaz (Undo/Redo’da dangling).
* **VERIFY BEFORE EXECUTE:** Her cached index doğrulanır; uyuşmazsa resolver full scan ile düzeltir.
* **BOUNDED BACKPRESSURE:** Queue’lar bounded; flood’a dayanıklı.

---

## 2. THREADING MODEL — The “Two-Queue” Architecture (Strict Message Passing)

### A) Network Thread (Background)

**Sadece:** TCP/WebSocket IO + connection bookkeeping
**Asla:** Reaper API, proje state okuma/yazma, track/FX tarama

**Channels**

* `Sender<InboundMsg>`  (Net -> Main)
* `Receiver<OutboundMsg>` (Main -> Net)

**Single-Client Policy (Bulletproof Default)**

* Aynı anda **tek aktif UI bağlantısı** desteklenir.
* Yeni bağlantı gelirse: **eski socket kapatılır**, yeni bağlantı “active” olur.
* Bu sayede “hangi socket?” belirsizliği tamamen biter.

**Session Token (Connection-Scoped)**

* Token’ı **Net Thread üretir** (random).
* Token yalnız aktif connection için geçerlidir.
* Net Thread, UI’dan gelen her JSON komutunda `session_token` kontrol eder:

  * Token yanlışsa → komut drop + `Error{code:"unauthorized"}` (mümkünse).

### B) Main Thread (Reaper Timer Loop)

**Sadece:** Reaper API, state caching, resolver, validation, apply batch
**Loop:** `plugin_register("timer")` ~30Hz

**Her tick sırası**

1. Inbound drain (ama “last-wins” ile)
2. Resolver/scan/validate
3. Apply (maks 1 SetTone batch)
4. Outbound push (Handshake/Ack/ProjectChanged/Error)

---

## 3. DATA STRUCTURES

### 3.1 Channels (Bounded + Backpressure)

```rust
use crossbeam_channel::{Sender, Receiver};

pub const INBOUND_CAP: usize = 256;
pub const OUTBOUND_CAP: usize = 256;

// Policy:
// - Inbound dolarsa: new command reddet (BUSY) veya drop-oldest (aşağıda netleştir)
// - Outbound dolarsa: non-critical mesajları drop (ProjectChanged gibi), critical olanları tut (Error/Ack)
```

**Inbound flood policy (Net Thread):**

* Queue doluysa:

  * `SetTone` için: **drop-oldest SetTone** veya **reject BUSY** (tercih: *reject BUSY* daha deterministik)
  * `RefreshInstances` için: **coalesce** (zaten tekrar refresh gereksiz)

### 3.2 Inbound / Outbound Msg

```rust
use std::net::SocketAddr;

pub enum InboundMsg {
    ClientConnected { socket_addr: SocketAddr, session_token: String },
    ClientDisconnected,
    Command { cmd: ClientCommand }, // cmd içinde session_token var
}

pub enum OutboundMsg {
    Send { msg: ServerMessage }, // Single-client: aktif client'a gönder, yoksa drop
}
```

### 3.3 Main Thread Cache (NO POINTERS)

```rust
use std::collections::HashMap;
use std::time::Instant;

pub struct GojiraCache {
    // Key: fx_guid (String)
    // Value: (track_guid (String), last_known_fx_index (i32))
    pub lookup: HashMap<String, (String, i32)>,

    pub last_project_change_count: i32,

    // Debounce
    pub last_broadcast_time: Instant,
    pub last_track_count: i32,
    pub last_total_fx_count: i32,

    // Optional: validator results cached per project
    pub validation_report: HashMap<String, String>,
}
```

---

## 4. STRICT JSON PROTOCOL (serde: snake_case + tagged enum)

> **Kural:** JSON mismatch olmasın diye serde casing “snake_case” ve `tag="type"` zorunlu.

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ServerMessage {
    Handshake {
        session_token: String,
        instances: Vec<GojiraInstance>,
        validation_report: std::collections::HashMap<String, String>,
    },
    ProjectChanged,
    Ack { command_id: String },
    Error { msg: String, code: ErrorCode },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ClientCommand {
    HandshakeAck {
        session_token: String
    },
    RefreshInstances {
        session_token: String
    },
    SetTone {
        session_token: String,
        command_id: String,
        target_fx_guid: String,
        mode: MergeMode,
        params: Vec<ParamChange>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    Merge,
    ReplaceActive,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParamChange {
    pub index: i32,
    pub value: f32,
}
```

---

## 5. REAPER API ACCESS RULE (ABSOLUTE)

**Yalnız Main Thread** şu fonksiyonları çağırır:

* Track/FX scan
* `TrackFX_GetFXGUID`, `TrackFX_GetFXName`
* `TrackFX_GetParamName`, `TrackFX_SetParam`
* `GetProjectStateChangeCount`, `CountTracks`, `GetTrack`, vs.

**Network Thread asla** Reaper API’ye dokunmaz.

---

## 6. CRITICAL LOGIC FLOWS

### 6.1 Connection & Handshake (Safe)

1. Net Thread: Accept → `session_token` üret → `InboundMsg::ClientConnected{token}`
2. Main Thread: `scan_project_instances()` + `validate_parameter_map()` çalıştırır
3. Main Thread: `OutboundMsg::Send{ ServerMessage::Handshake{token, instances, report} }`
4. Net Thread: Outbound’u socket’e yazar

---

## 7. TRACK RESOLUTION — “find_track_by_guid” (No imaginary API)

> **Kural:** `GetTrackByGUID` varsayma. Track’leri gezip GUID karşılaştır.

```rust
// MAIN THREAD ONLY
fn find_track_by_guid(track_guid: &str) -> Option<MediaTrack> {
    // pseudo:
    // for i in 0..CountTracks(0):
    //   let tr = GetTrack(0, i)
    //   let guid = GetTrackGUID(tr)
    //   if guid == track_guid => return Some(tr)
    None
}
```

---

## 8. THE “SMART RESOLVER” (Cache Invalidation Proof)

**Input:** `SetTone{target_fx_guid}`

1. Cache lookup: `lookup[fx_guid] -> (track_guid, idx)`
2. `find_track_by_guid(track_guid)` ile fresh track al

   * Yoksa → full scan + rebuild cache
3. Verify:

   * `TrackFX_GetFXGUID(track, idx)` == `target_fx_guid` ?
   * Eşleşmiyorsa → full scan + rebuild cache
4. Full scan sonrası 1 kez retry
5. Hâlâ yoksa → `Error{code: target_not_found}`

> **Kural:** Doğrulama geçmeden **asla** `SetParam` yok.

---

## 9. PARAMETER VALIDATION (Deterministic Heuristics)

**Amaç:** “Mix” isim çakışmalarında bile güvenilir eşleme.

**Anchor:**

* `delay.active` (expected 101) ve `reverb.active` (expected 112) isimleri doğrulanır.

**Delay Mix doğrulama:**

1. Sadece `100..115` bandında aday ara (anchor neighborhood)
2. `param_name` normalize: lowercase + whitespace/punct strip
3. “mix” içeren adayları topla
4. **101’e en yakın** adayı seç
5. Tie-break (yakınlık eşitse):

   * “feedback/time” komşuluğu olan bandı tercih et

**Report:**
Handshake içine:

* `"delay_mix": "confirmed at 107 (neighbor of 101)"`
* `"reverb_mix": "confirmed at 116 (neighbor of 112)"`

> Validator sadece **Handshake/Refresh** anında çalışır, her tick’te değil.

---

## 10. WATCHDOG — Debounced + Instance-Affecting Filter

**Sorun:** ProjectStateChangeCount çok gürültülü olabilir.
**Çözüm:** Debounce + “instance-affecting change” filtresi

Her tick:

1. `state = GetProjectStateChangeCount(0)`
2. Eğer `state != last_state` ise:

   * `track_count = CountTracks(0)`
   * `total_fx_count = sum TrackFX_GetCount(track)` (ucuz hesapla)
   * Sadece `track_count` veya `total_fx_count` değiştiyse “instances affected” say
3. Eğer “instances affected” ve `now - last_broadcast > 500ms` ise:

   * `OutboundMsg::Send{ProjectChanged}`

---

## 11. APPLY RULES — Safety + “Last-Wins Atomicity”

### 11.1 Sanitization (Main Thread)

Her `ParamChange` için:

* `if !value.is_finite()` → reject
* `value = clamp(0.0..1.0)`
* `index < 0` veya index aşırı büyükse → reject/log

### 11.2 Last-Wins

Timer tick’te inbound drain yapılır ama:

* Bir tick içinde gelen **birden fazla SetTone** varsa, **yalnız son SetTone** uygulanır.
* `RefreshInstances` komutları coalesce edilir (1 kez yeter).

Bu sayede “yarım preset” hissi azalır.

---

## 12. SMART CLEANER — ReplaceActive (Non-Destructive Bypass)

`mode == replace_active` ise:

* “Touched modules” set’i üret:

  * Bir module’a ait **active dışındaki** paramlardan biri geliyorsa touched say.
* Touched olmayan her module için:

  * Sadece `{ active_index, 0.0 }` enjekte et
* Knob reset yok (time/mix değerleri korunur)

---

## 13. IDENTITY CHECK (Scan)

Scan sırasında:

* `TrackFX_GetFXName` normalize edilir
* “archetype” + “gojira” fuzzy match ile doğrulanır
* Şüpheli durumda instance listesine “confidence: low” notu eklenebilir (opsiyonel)

---

## 14. GRACEFUL SHUTDOWN (No Port Holding)

* DLL unload / `plugin_register("-timer")` anında:

  * `shutdown_flag.store(true)`
  * Network thread accept loop çıkıp socket’i kapatır
* Non-blocking accept + kısa sleep/backoff ile temiz kapanış

---

## 15. EXECUTION ORDER (Do Not Deviate)

1. Workspace + crates
2. Two-Queue threading + bounded channels + single-client + token
3. Main thread scan + handshake + UI instance selection
4. Resolver + cache (track_guid + idx) + verify + full scan retry
5. Watchdog (debounced + instance-affecting)
6. apply_batch (sanitization + last-wins) + ReplaceActive cleaner

**AGENT INSTRUCTION:**
Önce bağlantı/handshake/instance seçimi “uçtan uca” çalışacak. Parametre set etmeye ancak resolver + validation report doğrulandıktan sonra geç.
