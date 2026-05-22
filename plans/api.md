# REST Client

Replaces: Postman, Insomnia, Bruno

## Storage

```
.anide/requests/
  collection-name/          # folder = collection
    request-name.md         # one file per request
  standalone-request.md     # top-level requests allowed
  .history/                 # gitignored; last N responses per request
    request-name/
      2024-01-15T10-32-00.json
```

## Request file format

YAML frontmatter + markdown body. The body section after `---` is freeform markdown
used for notes and the request body template.

```markdown
---
method: POST
url: "{{BASE_URL}}/api/users"
headers:
  Content-Type: application/json
  Authorization: "Bearer {{env.staging.TOKEN}}"
  X-Request-ID: "{{Faker.datatype.uuid}}"
params:
  - key: page
    value: "1"
    enabled: true
  - key: limit
    value: "20"
    enabled: true
auth:
  type: bearer
  token: "{{env.API_TOKEN}}"
body_type: json
---
# Create User

Creates a new user account. Requires admin token.

## Body

```json
{
  "email": "{{Faker.internet.email}}",
  "name": "{{Faker.name.fullName}}",
  "role": "user",
  "metadata": {
    "source": "anide",
    "request_id": "{{Faker.datatype.uuid}}"
  }
}
```
```

### Frontmatter fields

| Field | Type | Description |
|---|---|---|
| `method` | string | `GET` `POST` `PUT` `PATCH` `DELETE` `HEAD` `OPTIONS` |
| `url` | string | Full URL; template tokens resolved at send time |
| `headers` | map | Key-value pairs; values support templates |
| `params` | array of `{key, value, enabled}` | Query parameters |
| `auth` | object | See auth types below |
| `body_type` | string | `json` `form` `multipart` `raw` `graphql` `none` |

### Auth types

```yaml
# None
auth:
  type: none

# Bearer / JWT
auth:
  type: bearer
  token: "{{env.API_TOKEN}}"

# Basic
auth:
  type: basic
  username: "{{env.API_USER}}"
  password: "{{env.API_PASS}}"

# API Key — header or query
auth:
  type: apikey
  key: X-API-Key
  value: "{{env.API_KEY}}"
  add_to: header   # or: query

# OAuth 2.0 Client Credentials
auth:
  type: oauth2
  grant_type: client_credentials
  token_url: "{{env.TOKEN_URL}}"
  client_id: "{{env.CLIENT_ID}}"
  client_secret: "{{env.CLIENT_SECRET}}"
  scope: "read write"
```

---

## Features

### Collections sidebar

- Tree view of `.anide/requests/` — folders are collections, `.md` files are requests.
- Create folder / create request / rename / delete / duplicate (all via context menu).
- Drag to reorder within a collection.
- Search bar filters requests by name or method.

### Request editor

**URL bar**
- Method dropdown (colored badge per method).
- URL input with live variable preview: hovering a `{{TOKEN}}` shows its resolved value.
- Unresolved tokens highlighted red.
- Send button (`Ctrl+Enter`).

**Tabs below URL**

| Tab | Content |
|---|---|
| Params | Key-value table for query parameters; enable/disable per row |
| Headers | Key-value table; inherited headers from auth shown greyed out |
| Auth | Auth type picker + fields; fills headers automatically |
| Body | Body type picker + editor (see below) |
| Variables | Inspector showing all `{{...}}` tokens, source, resolved value |

**Body editor types**
- `json` — CodeMirror with JSON syntax highlighting + auto-format.
- `form` — Key-value table, URL-encoded on send.
- `multipart` — Key-value table with file picker per row.
- `raw` — Plain textarea with content-type selector.
- `graphql` — Query + variables panes; introspection support.
- `none` — No body.

**Markdown notes**
- The markdown body of the `.md` file (below the frontmatter) is shown as a collapsible
  notes panel — rendered as HTML for reading, editable as raw markdown.

### Response panel

| Section | Content |
|---|---|
| Status bar | HTTP status code + text, time (ms), size (bytes) |
| Body | Syntax-highlighted JSON/XML/HTML; raw toggle; copy button |
| Headers | Response headers table |
| Cookies | Parsed cookies table |
| Timeline | DNS → Connect → TLS → Send → Wait → Receive breakdown |

### History

- Last 50 responses per request stored in `.anide/requests/.history/request-name/`.
- Each entry: timestamp, status, duration, request snapshot (resolved), response.
- History panel in sidebar: click any entry to replay or compare with current.

### Environment switcher

- Dropdown in the top bar to select the "active" env file set.
- Options come from the `.env*` files scanned by the existing project watcher.
- Active selection persisted in `.anide/settings.json` (not committed).
- Variable Inspector updates live when env changes.

### Import

| Source | Format |
|---|---|
| Postman | Collection JSON v2 / v2.1 |
| OpenAPI | YAML or JSON, v3.x |
| curl | Paste curl command string |

Import creates `.md` files under a new collection folder named after the source.

---

## Tauri commands (Rust)

Existing commands in `api.rs` cover file CRUD and tree operations. New commands needed:

### `send_request`

```rust
// Input
struct SendRequestArgs {
    project_path: String,
    request: RequestData,       // method, url, headers, params, auth, body
    env_context: EnvContext,    // { active_files: Vec<String>, overrides: HashMap }
    follow_redirects: bool,
    timeout_ms: u32,
}

// Output
struct RequestResponse {
    status: u16,
    status_text: String,
    headers: Vec<KVPair>,
    body: String,               // raw response body
    duration_ms: u64,
    size_bytes: usize,
    redirects: Vec<String>,
    request_snapshot: RequestData,  // the resolved request (for history)
}
```

Resolution happens in Rust:
1. Load env files listed in `env_context.active_files` (reuse `read_env_file`).
2. Parse each into `HashMap<String, String>`.
3. Walk all template strings in `RequestData`, substitute `{{...}}` tokens.
4. Faker tokens are resolved **before** hitting Rust — the frontend substitutes them first.
5. Send via `reqwest` (async, with TLS via `rustls`).

### `resolve_template`

```rust
// Preview a single template string — used for the variable inspector
fn resolve_template(template: String, env_context: EnvContext) -> Result<ResolvedTemplate>
// ResolvedTemplate: { resolved: String, tokens: Vec<TokenInfo> }
// TokenInfo: { token: String, source: String, value: Option<String> }
```

### `import_collection`

```rust
fn import_collection(
    project_path: String,
    source_type: String,  // "postman" | "openapi" | "curl"
    content: String,      // raw JSON/YAML/curl string
    collection_name: String,
) -> Result<Vec<String>>  // paths of created files
```

### Rust deps to add

```toml
reqwest = { version = "0.12", features = ["json", "rustls-tls", "multipart", "stream"] }
# reqwest replaces any placeholder HTTP client
```

---

## Frontend (Svelte)

### New files

```
src/lib/components/workspace/ApiTab.svelte      # main request/response editor
src/lib/components/panels/ApiPanel.svelte       # already exists, currently stub
src/lib/commands/api.js                         # already exists, add sendRequest + resolveTemplate
```

### ApiPanel.svelte (sidebar)

- Collection tree using the existing tree data from `list_api_tree` command.
- Click request → `workspace.openTab({ type: 'api-request', data: { relPath } })`.
- Toolbar: New Request, New Folder, Import, Search.

### ApiTab.svelte (workspace tab)

- Loads request file via `read_api_request(path)`.
- Two-pane layout: request editor (top/left) + response (bottom/right), resizable.
- Calls `send_request` on send; writes history entry; shows response.
- Dirty state tracked in `workspace.dirtyTabIds`.
- Save (`Ctrl+S`) calls `update_api_request`.

### Template resolution in frontend

```js
import { faker } from '@faker-js/faker'

function resolveFakerTokens(template) {
  return template.replace(/\{\{Faker\.([^}]+)\}\}/g, (_, path) => {
    const parts = path.split('.')
    // e.g. ["internet", "email"] or ["number", "int", '{"min":1,"max":100}']
    let obj = faker
    for (const part of parts.slice(0, -1)) obj = obj[part]
    const method = parts.at(-1).replace(/\(.*\)$/, '')
    const argsMatch = path.match(/\((.+)\)$/)
    const args = argsMatch ? [JSON.parse(argsMatch[1])] : []
    return String(obj[method](...args))
  })
}
```

Faker tokens are resolved in the frontend before the resolved string is passed to
`send_request`. This keeps Faker out of Rust entirely.
