# Database Browser

Replaces: DBeaver, TablePlus, DataGrip

## Storage

Each connection gets its own folder. `config.md` holds the connection details; `queries/`
holds saved queries for that connection. Both are committed to the repo.

```
.anide/database/
  local-postgres/
    config.md
    queries/
      get-active-users.md
      monthly-revenue.md
  prod-mysql/
    config.md
    queries/
      find-orders.md
  staging-sqlite/
    config.md
  analytics-mongo/
    config.md
    queries/
      pipeline-revenue.md
```

## config.md format

Lives at `.anide/database/<connection-name>/config.md`.

```markdown
---
name: "Local Postgres"
type: postgresql
host: "{{env.DB_HOST}}"
port: "{{env.DB_PORT}}"
database: "{{env.DB_NAME}}"
username: "{{env.DB_USER}}"
password: "{{env.DB_PASS}}"
ssl: false
# Alternative: use a single connection string
# connection_string: "postgresql://{{env.DATABASE_URL}}"
---
# Local development database

Postgres 16 running in Docker via docker-compose.
```

## Query file format

Lives at `.anide/database/<connection-name>/queries/<query-name>.md`.
Frontmatter carries name and description; the markdown body is the raw SQL
(or MongoDB aggregation JSON for Mongo connections).

```markdown
---
name: "Get Active Users"
description: "Returns all users active in the last 30 days, ordered by recency"
---
SELECT
  id,
  email,
  last_active_at
FROM users
WHERE last_active_at > NOW() - INTERVAL '30 days'
ORDER BY last_active_at DESC
LIMIT 100
```

```markdown
---
name: "Monthly Revenue"
description: "Sum of completed orders grouped by calendar month"
---
SELECT
  DATE_TRUNC('month', completed_at) AS month,
  SUM(total_cents) / 100.0          AS revenue_usd
FROM orders
WHERE status = 'completed'
GROUP BY 1
ORDER BY 1 DESC
```

MongoDB aggregation example:

```markdown
---
name: "Revenue Pipeline"
description: "Aggregate revenue by product category"
---
[
  { "$match": { "status": "completed" } },
  { "$group": { "_id": "$category", "total": { "$sum": "$amount" } } },
  { "$sort": { "total": -1 } }
]
```

### Supported types

| Value | Database |
|---|---|
| `postgresql` | PostgreSQL 12+ |
| `mysql` | MySQL 8+ / MariaDB 10+ |
| `sqlite` | SQLite 3 (path instead of host) |
| `mongodb` | MongoDB 6+ |

### SQLite fields

```yaml
---
name: "Local SQLite"
type: sqlite
path: "{{env.SQLITE_PATH}}"   # absolute or relative to project root
---
```

### MongoDB fields

```yaml
---
name: "Atlas Dev"
type: mongodb
connection_string: "mongodb+srv://{{env.MONGO_USER}}:{{env.MONGO_PASS}}@{{env.MONGO_HOST}}/{{env.MONGO_DB}}"
---
```

---

## Features

### Connection manager (sidebar panel)

- List all `.anide/database/*.md` connections.
- Status indicator: connected (green) / disconnected (grey) / error (red).
- Buttons: Connect, Disconnect, Test, Edit, Duplicate, Delete.
- "Test connection" resolves templates, pings the DB, shows latency or error message.
- New connection wizard: type picker → fill fields → test → save.

### Schema browser

Tree structure inside a connected DB tab:

```
▾ local-postgres
  ▾ public (schema)
    ▾ users (table)
        id          bigserial   PK
        email       varchar(255) UNIQUE
        created_at  timestamptz
    ▾ posts (table)
        id          bigserial   PK
        user_id     bigint      FK → users.id
        ...
  ▾ auth (schema)
    ▾ sessions (table)
        ...
  ▸ Views
  ▸ Functions
  ▸ Indexes
```

Clicking a table opens a data tab (first 100 rows, paginated).

### Query editor tab

- CodeMirror 6 with SQL language support + syntax highlighting.
- Run selection or entire editor with `Ctrl+Enter`.
- Multiple editor tabs per connection (named "Query 1", "Query 2", etc.).
- Query history: last 200 queries per connection stored in memory (session only).
- Explain plan: `Ctrl+Shift+Enter` runs `EXPLAIN ANALYZE` and shows visual plan.

### Results table

- Column headers: type badge, sortable (click to sort, shift-click multi-sort).
- Cells: click to copy value; long text truncated with expand-on-hover.
- Inline row editing: double-click a cell → editable input → `Enter` to commit
  (generates and runs `UPDATE ... WHERE id = ?`).
- Toolbar: Export CSV, Export JSON, Copy all as JSON, row count.
- NULL displayed as `NULL` (styled differently from empty string).
- Pagination: 100 rows per page, offset-based.

### Data tab (table viewer)

Opened by clicking a table in the schema browser.

- Full results table (see above) pre-populated with `SELECT * FROM table LIMIT 100`.
- Filter bar: add column conditions without writing SQL.
- Add row button: opens inline empty row at top, fills on commit.
- Delete row: select row(s) → Delete key → confirmation dialog.

### Query history

- Stored per connection in `.anide/database/.history/connection-name.json`.
- Shows: query text (truncated), timestamp, row count, duration.
- Click to paste into editor. Gitignored.

### Migration tracking

- Scans project for `*.sql` files (configurable pattern, default: `migrations/*.sql`).
- Tracks applied migrations in a `_anide_migrations` table created on first use.
- UI: list migrations with status (applied / pending), applied-at timestamp.
- Run: execute selected pending migration, record in tracking table.
- Rollback: not managed automatically — shows warning to write rollback manually.

---

## Tauri commands (Rust)

### State

```rust
// Added to AppState
struct DbState {
    connections: Mutex<HashMap<String, DbConnection>>,
}

enum DbConnection {
    Postgres(sqlx::PgPool),
    Mysql(sqlx::MySqlPool),
    Sqlite(sqlx::SqlitePool),
    Mongo(mongodb::Client),
}
```

### Commands

```rust
db_list_connections(project_path: String) -> Result<Vec<ConnectionMeta>>
db_save_connection(project_path: String, conn: ConnectionData) -> Result<()>
db_delete_connection(project_path: String, name: String) -> Result<()>

db_test_connection(project_path: String, conn: ConnectionData, env_context: EnvContext)
  -> Result<TestResult>   // { latency_ms: u64, server_version: String }

db_connect(project_path: String, connection_name: String, env_context: EnvContext)
  -> Result<()>   // stores pool in DbState

db_disconnect(project_path: String, connection_name: String) -> Result<()>

db_get_schema(project_path: String, connection_name: String)
  -> Result<SchemaTree>   // { schemas: [{ name, tables: [{ name, columns: [...] }] }] }

db_query(project_path: String, connection_name: String, sql: String)
  -> Result<QueryResult>  // { columns: [{ name, type_name }], rows: Vec<Vec<Value>>, affected: Option<u64>, duration_ms: u64 }

db_update_row(project_path: String, connection_name: String,
              table: String, pk_column: String, pk_value: Value,
              column: String, new_value: Value) -> Result<()>

db_insert_row(project_path: String, connection_name: String,
              table: String, values: HashMap<String, Value>) -> Result<()>

db_delete_rows(project_path: String, connection_name: String,
               table: String, pk_column: String, pk_values: Vec<Value>) -> Result<()>

db_list_migrations(project_path: String, connection_name: String) -> Result<Vec<MigrationStatus>>
db_run_migration(project_path: String, connection_name: String, file_path: String) -> Result<()>
```

### Rust deps to add

```toml
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "mysql", "sqlite", "json", "chrono", "uuid"] }
mongodb = { version = "3", features = ["tokio-runtime"] }
```

---

## Frontend (Svelte)

### New files

```
src/lib/components/workspace/DbTab.svelte          # query editor + results
src/lib/components/workspace/DbTableTab.svelte     # table data viewer
src/lib/components/panels/DbPanel.svelte           # connection list + schema tree
src/lib/commands/db.js                             # Tauri command wrappers
```

### DbPanel.svelte (sidebar)

- Accordion list: each connection is a collapsible section.
- Expanded: schema tree (lazy-loaded on first expand).
- Click table → `workspace.openTab({ type: 'db-table', data: { connection, table, schema } })`.
- "New Query" button → `workspace.openTab({ type: 'db-query', data: { connection } })`.
- Connection status dot + connect/disconnect button.

### DbTab.svelte (query editor)

- CodeMirror 6 SQL editor (top pane, resizable).
- Results table (bottom pane).
- Toolbar: Run, Explain, Format SQL, History dropdown, Export.
- Multiple SQL statements: run all or run statement at cursor.

### DbTableTab.svelte (table data)

- Toolbar: table name breadcrumb, Add Row, Export, Filter.
- Full results table with inline editing.
- Pagination controls.

### Value serialisation

`QueryResult.rows` uses a `Value` enum serialised to JSON:

```
null → null
bool → boolean
integer → number
float → number
text → string
bytes → { "$bytes": "<base64>" }
date/time → ISO 8601 string
json → object/array (parsed)
```

Frontend renders each type with appropriate formatting.
