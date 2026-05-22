# Cache / KV Browser

Replaces: Redis Insight, Another Redis Desktop Manager, Medis

Supports: Redis, Valkey, KeyDB, Dragonfly

## Storage

Each connection gets its own folder. `config.md` holds the connection details; `queries/`
holds saved commands — SCAN patterns, Lua scripts, or raw multi-command sequences.

```
.anide/kv/
  local-redis/
    config.md
    queries/
      flush-sessions.md
      count-user-keys.md
      atomic-counter-reset.md
  staging-valkey/
    config.md
    queries/
      warm-cache.md
```

## config.md format

Lives at `.anide/kv/<connection-name>/config.md`.

```markdown
---
name: "Local Redis"
type: redis   # redis | valkey | keydb | dragonfly
host: "{{env.REDIS_HOST}}"
port: "{{env.REDIS_PORT}}"
password: "{{env.REDIS_PASSWORD}}"
db: 0
tls: false
# Alternative: connection URL
# url: "redis://:{{env.REDIS_PASSWORD}}@{{env.REDIS_HOST}}:{{env.REDIS_PORT}}/0"
---
# Local Redis

Running in Docker. Password in .env.local.
```

TLS / Redis Cloud:

```markdown
---
name: "Redis Cloud"
type: redis
url: "rediss://{{env.REDIS_CLOUD_URL}}"
tls: true
---
```

## Query file format

Lives at `.anide/kv/<connection-name>/queries/<query-name>.md`.
Three query types determined by the `type` frontmatter field.

### Scan pattern

A saved key search. Opened in the key browser with the pattern pre-filled.

```markdown
---
name: "User Sessions"
description: "Find all active user session keys"
type: scan
---
session:user:*
```

### Lua script

Executed via `EVAL`. `KEYS` and `ARGV` injected at run time through a params UI.

```markdown
---
name: "Atomic Counter Reset"
description: "Atomically GET and reset a counter to 0. KEYS[1] = counter key."
type: lua
---
local current = redis.call('GET', KEYS[1])
redis.call('SET', KEYS[1], 0)
return current
```

### Command sequence

Ordered list of Redis commands run sequentially (not atomic). Each line is one command.

```markdown
---
name: "Flush Session Keys"
description: "Delete all session:* keys using SCAN + DEL in batches"
type: commands
---
SCAN 0 MATCH session:* COUNT 100
DEL {keys}
```

The `{keys}` placeholder is filled by the UI from the SCAN result before executing DEL.

---

## Features

### Connection manager (sidebar panel)

- List connections from `.anide/kv/*.md`.
- Status: connected / disconnected / error.
- Connect / Disconnect / Test / Edit / Delete.
- Test shows: ping latency, Redis version, used memory, connected clients.

### Key browser

Main view when connected. Left side: key list. Right side: value viewer.

**Key list**
- Shows keys in pages of 200 (SCAN-based, not KEYS — safe on production).
- Search: pattern filter (glob, e.g. `user:*`), applied via SCAN `MATCH`.
- Group by prefix: keys with `:` separator shown as collapsible folders
  (`user:` → `user:1`, `user:2`, ...).
- Per-key badges:
  - Type: `STRING` `HASH` `LIST` `SET` `ZSET` `STREAM` (colour-coded)
  - TTL: countdown (`5m`, `2h`, `∞`); colour: green → yellow (< 60s) → red (expired/missing)
- Multi-select with checkbox for bulk delete.
- Refresh button + auto-refresh toggle (every 5s).

**Value viewer (right pane)**

Adapts to key type:

| Type | Display | Edit |
|---|---|---|
| `STRING` | Raw value (JSON pretty-printed if valid JSON) | Inline text edit |
| `HASH` | Field-value table | Add/edit/delete fields |
| `LIST` | Ordered list with index | Append, prepend, insert at index, delete by index |
| `SET` | Unordered member list | Add member, remove member |
| `ZSET` | Member + score table (sorted) | Add member+score, update score, remove member |
| `STREAM` | Entry table (ID, fields) | Add entry (`XADD`) |

Value viewer header bar: key name, type badge, TTL countdown + set TTL button, Delete key button.

**Key info panel** (collapsible at bottom)

```
Type:      STRING
Encoding:  embstr
TTL:       300s (expires 2024-01-15 10:35:00)
Idle:      12s
Refcount:  1
Size:      47 bytes
```

### TTL editor

Click the TTL badge → popover:
- Slider + input for seconds.
- Presets: 5m, 1h, 24h, 7d, 30d, Never.
- `PERSIST` button to remove TTL.
- Confirms before applying.

### Pub/Sub debugger

Separate tab (or panel section) for pub/sub:

- **Subscribe**: enter channel pattern (supports glob `*`), click Subscribe.
  Messages appear in a live feed (timestamp, channel, message, type `message`/`pmessage`).
- **Publish**: enter channel + message, click Publish. Shows reply count.
- Multiple active subscriptions listed; each can be unsubscribed individually.

### Flush tool

"Flush Keys" button (in toolbar, clearly dangerous):
- Pattern input (default `*` = all keys in current DB).
- Preview: shows count of matching keys (SCAN + COUNT, no delete yet).
- Type confirmation string (`"delete N keys"`) before enabling the button.
- Executes: SCAN + DEL in batches of 100.

---

## Tauri commands (Rust)

### State

```rust
struct KvState {
    connections: Mutex<HashMap<String, redis::aio::MultiplexedConnection>>,
}
```

### Commands

```rust
kv_list_connections(project_path: String) -> Result<Vec<ConnectionMeta>>
kv_save_connection(project_path: String, conn: KvConnectionData) -> Result<()>
kv_delete_connection(project_path: String, name: String) -> Result<()>

kv_test_connection(project_path: String, conn: KvConnectionData, env_context: EnvContext)
  -> Result<KvServerInfo>   // { version, used_memory_human, connected_clients, latency_ms }

kv_connect(project_path: String, connection_name: String, env_context: EnvContext)
  -> Result<KvServerInfo>

kv_disconnect(project_path: String, connection_name: String) -> Result<()>

// Key browsing (SCAN-based)
kv_scan_keys(project_path: String, connection_name: String,
             pattern: String, cursor: u64, count: u32)
  -> Result<ScanResult>   // { cursor: u64, keys: Vec<KeyMeta> }
// KeyMeta: { key: String, type: String, ttl: i64 }   ttl: -1 = no expire, -2 = gone

kv_get_key(project_path: String, connection_name: String, key: String)
  -> Result<KvValue>   // tagged union per type

kv_set_string(project_path: String, connection_name: String,
              key: String, value: String, ttl_secs: Option<i64>) -> Result<()>

kv_set_ttl(project_path: String, connection_name: String,
           key: String, ttl_secs: Option<i64>) -> Result<()>   // None = PERSIST

kv_delete_keys(project_path: String, connection_name: String,
               keys: Vec<String>) -> Result<u64>   // count deleted

kv_get_key_info(project_path: String, connection_name: String, key: String)
  -> Result<KeyInfo>   // type, encoding, ttl, idle, refcount, serializedlength

// Hash operations
kv_hset(project_path, connection_name, key, field, value) -> Result<()>
kv_hdel(project_path, connection_name, key, field) -> Result<()>

// List operations
kv_lpush / kv_rpush(project_path, connection_name, key, value) -> Result<i64>
kv_lset(project_path, connection_name, key, index, value) -> Result<()>
kv_lrem(project_path, connection_name, key, index) -> Result<()>   // by index via LSET + LREM trick

// Set operations
kv_sadd(project_path, connection_name, key, member) -> Result<i64>
kv_srem(project_path, connection_name, key, member) -> Result<i64>

// Sorted set operations
kv_zadd(project_path, connection_name, key, score, member) -> Result<i64>
kv_zrem(project_path, connection_name, key, member) -> Result<i64>

// Stream operations
kv_xadd(project_path, connection_name, key, fields: HashMap<String, String>) -> Result<String>

// Flush
kv_count_pattern(project_path, connection_name, pattern) -> Result<u64>
kv_flush_pattern(project_path, connection_name, pattern) -> Result<u64>

// Pub/sub — long-lived; uses Tauri events for streaming
kv_subscribe(project_path, connection_name, pattern) -> Result<String>  // returns subscription_id
kv_unsubscribe(project_path, connection_name, subscription_id) -> Result<()>
kv_publish(project_path, connection_name, channel, message) -> Result<i64>
```

Pub/sub messages emitted as Tauri events: `kv-pubsub-{subscription_id}`.

### Rust deps to add

```toml
redis = { version = "0.27", features = ["tokio-comp", "connection-manager", "streams"] }
```

---

## Frontend (Svelte)

### New files

```
src/lib/components/workspace/KvTab.svelte       # key browser + value viewer
src/lib/components/workspace/KvPubSubTab.svelte # pub/sub debugger
src/lib/components/panels/KvPanel.svelte        # connection list
src/lib/commands/kv.js                          # Tauri command wrappers
```

### KvPanel.svelte (sidebar)

- Connection list with status dot.
- Click connected connection → open KvTab.
- "Pub/Sub" button → open KvPubSubTab.
- New / Edit / Delete connection actions.

### KvTab.svelte (main browser)

- Left pane: SCAN-based key list with infinite scroll (load more on scroll to bottom).
- Search input at top triggers new SCAN with `MATCH pattern*`.
- Right pane: KvValueViewer component (adapts per type).
- Bottom: KeyInfo panel (collapsible).

### KvValueViewer component

One component with `{#if type === 'string'}...{:else if type === 'hash'}...` branches.
Edits call the appropriate `kv_*` commands and refresh the local value.

### TTL colour logic

```js
function ttlColor(ttl) {
  if (ttl === -1) return 'text-muted-foreground'   // no expire
  if (ttl <= 0)   return 'text-red-500'            // expired
  if (ttl <= 60)  return 'text-yellow-500'         // expiring soon
  return 'text-green-500'
}
```
