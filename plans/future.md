# Future Features

Ideas beyond the current roadmap that would meaningfully improve the developer experience.
Not prioritised for now — captured here so nothing gets lost.

---

## SSH Tunnel Manager

**Why:** DB/KV/S3 connections in real projects almost never have ports exposed directly.
Without tunnels, those tools only work locally — which cuts out the majority of real use cases.

Storage: `.anide/tunnels/<name>/config.md`

```markdown
---
name: "Prod Postgres Tunnel"
host: "{{env.SSH_HOST}}"
user: "{{env.SSH_USER}}"
identity_file: "{{env.SSH_KEY_PATH}}"
local_port: 5433
remote_host: "127.0.0.1"
remote_port: 5432
---
# Production database tunnel
Connects through bastion to internal RDS instance.
```

DB/KV/S3 connections can reference a tunnel by name (`tunnel: "prod-postgres-tunnel"`).
The app starts the tunnel automatically when the connection is opened and tears it down on
disconnect.

---

## Log File Viewer

**Why:** Docker logs are covered, but most apps also write to files — `logs/app.log`,
Rails log, Nginx access log, etc. Devs constantly have a `tail -f` running somewhere.

- Watch any file in the project for appended lines (uses existing `notify-debouncer` infra).
- Live stream new lines into a virtual-scrolled viewer (same pattern as DockerLogsTab).
- Regex filter — only show lines matching a pattern.
- JSON log parsing — detect JSON lines and pretty-print them with collapsible fields.
- Log level highlighting — auto-detect ERROR/WARN/INFO/DEBUG and colour accordingly.
- Pause/resume stream; jump to bottom.

Storage: saved log file paths in `.anide/logs/config.md` (optional, for quick re-open).

---

## Port Manager / Local Services

**Why:** Every dev spends time asking "what the hell is on port 3000?" — kill it, find the
process, figure out if it's a zombie Docker container or a stale node process.

- Live list of TCP ports in use on the machine: port, PID, process name, state.
- One-click kill (sends SIGTERM, confirm dialog).
- Surface Docker port mappings alongside OS-level listeners so everything is in one view.
- Mark ports as "expected" (config saved in `.anide/ports.md`) — anything unexpected
  is highlighted.

No storage needed beyond optional expected-port config.

---

## JWT / Token Inspector

**Why:** Used constantly — checking token expiry, debugging auth issues, verifying claims.
Small feature, high daily utility.

- Paste any JWT string → decoded header, payload (pretty-printed), signature status.
- Expiry countdown with colour (green / yellow / red).
- Auto-detect bearer tokens in REST client auth headers and response bodies — click the
  token chip to open the inspector inline.
- Supports JWTs and opaque tokens (show raw base64 decode).
- No secret key needed for decode (inspection only, not verification by default).
  Optional: paste JWKS URL or public key to verify signature.

Lives as a utility panel, not a full tab — accessible from the sidebar or as a popover
in the REST client.

---

## Mock Server

**Why:** Frontend devs need a backend before the real one exists. Also essential for
testing error states, slow responses, and edge cases that are hard to trigger against a
real API.

Storage: `.anide/mocks/<name>.md`

```markdown
---
method: POST
path: /api/users
status: 201
delay_ms: 0
headers:
  Content-Type: application/json
---
{
  "id": "{{Faker.datatype.uuid}}",
  "email": "{{Faker.internet.email}}",
  "created_at": "{{Faker.date.recent}}"
}
```

- Start/stop a local HTTP server from the UI (configurable port).
- Hot-reload on file save — no restart needed.
- Request log: see every incoming request, matched rule, response sent.
- Faker tokens in response bodies re-evaluated on each request.
- Passthrough mode: if no mock matches, proxy to a real upstream URL.
- Import from existing REST client requests (create a mock that mirrors the saved response).

---

## AI Features in Tools

**Why:** The product is positioned as AI-native but the current plans describe a polished
GUI client. The actual differentiator is AI woven into the tools themselves.

### Query editor (DB)
- Natural language → SQL: type "show me users who signed up last week" → generates SQL.
- "Explain this result": highlight a result set → AI summarises what the data means.
- "Optimise this query": AI suggests indexes, rewrites, or spots N+1 patterns.

### Log viewer
- "Explain this error": select a log line or stack trace → AI explains the root cause
  and suggests fixes.
- Auto-group repeated errors and surface a summary.

### REST client
- "Generate body from schema": AI reads the OpenAPI spec (if imported) and suggests a
  valid request body.
- "Explain this response": summarise what the API returned in plain English.
- Detect breaking changes between two responses (response diff + AI summary).

### Database
- Generate seed data: AI reads the table schema and produces INSERT statements with
  realistic fake values (goes further than Faker — understands FK relationships).
- "What does this table do?": AI summarises the schema and any FK relationships.

### General
- All AI calls go through the app's Claude integration — no separate API key UX needed
  if the user already has Claude Code configured.

---

## Multi-environment Diff

**Why:** "Why does this work in staging but not prod?" is a daily question. The building
blocks (env files, REST client, DB) are already planned — just needs a diff surface.

- **Env diff**: side-by-side compare any two `.env.*` files, highlight keys present in
  one but not the other, values that differ.
- **Response diff**: run the same REST client request against two base URLs (e.g. staging
  vs prod env), diff the response bodies.
- **Query result diff**: run the same saved DB query against two connections, diff the
  result sets (useful for verifying migrations).

Accessible from the respective tool's toolbar: "Compare with..." picker.
