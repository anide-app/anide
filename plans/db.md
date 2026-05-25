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

> **History deferred:** Request/query/result history will be added later using DuckDB or SQLite
> as a local store. Do not implement `.history/` now — focus on live functionality only.

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

### Supported types (initial)

| Value | Database |
|---|---|
| `postgresql` | PostgreSQL 12+ |
| `mysql` | MySQL 8+ / MariaDB 10+ |
| `sqlite` | SQLite 3 (path instead of host) |
| `mongodb` | MongoDB 6+ |

The driver system is modular — adding new databases (ClickHouse, BigQuery, Snowflake, etc.)
means implementing one Rust trait and registering it. See **Modular Driver Architecture** below.

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

## Modular Driver Architecture

Every database type is a Rust module implementing a single trait. Adding a new DB or data
warehouse (ClickHouse, BigQuery, Snowflake, DuckDB, etc.) means dropping in a new module with
no changes to the rest of the codebase.

```rust
/// Core trait every database driver must implement.
/// All methods are async. Schema concepts adapt per DB type.
#[async_trait]
pub trait DbDriver: Send + Sync {
    /// Human-readable name shown in UI ("PostgreSQL", "MongoDB", ...)
    fn display_name(&self) -> &'static str;

    /// The `type` value in config.md
    fn config_type(&self) -> &'static str;

    /// What the tree calls the top-level grouping below the connection.
    /// "database" for relational, "database" for Mongo (same concept).
    fn tree_structure(&self) -> TreeStructure;

    async fn connect(&self, config: &ConnectionConfig) -> Result<Box<dyn DbConnection>>;
    async fn test(&self, config: &ConnectionConfig) -> Result<TestResult>;
}

/// Describes how the sidebar tree is shaped for this DB type.
pub struct TreeStructure {
    pub levels: Vec<TreeLevel>,  // e.g. Database > Schema > Table > Column
}

pub struct TreeLevel {
    pub label: &'static str,     // "Schema", "Collection", "Dataset", ...
    pub icon: &'static str,      // icon key used in frontend
    pub children: Vec<TreeLevel>,
}

/// A live, authenticated connection to one database instance.
#[async_trait]
pub trait DbConnection: Send + Sync {
    async fn list_databases(&self) -> Result<Vec<String>>;
    async fn list_schemas(&self, database: &str) -> Result<Vec<String>>;
    async fn list_tables(&self, database: &str, schema: &str) -> Result<Vec<TableMeta>>;
    async fn list_columns(&self, database: &str, schema: &str, table: &str) -> Result<Vec<ColumnMeta>>;
    async fn list_indexes(&self, database: &str, schema: &str, table: &str) -> Result<Vec<IndexMeta>>;
    async fn list_views(&self, database: &str, schema: &str) -> Result<Vec<String>>;
    async fn list_functions(&self, database: &str, schema: &str) -> Result<Vec<FunctionMeta>>;
    async fn get_relationships(&self, database: &str, schema: &str) -> Result<Vec<Relationship>>;

    async fn query(&self, sql: &str) -> Result<QueryResult>;
    async fn query_page(&self, sql: &str, offset: u64, limit: u32) -> Result<QueryResult>;
    async fn count_table(&self, database: &str, schema: &str, table: &str) -> Result<u64>;

    async fn update_row(&self, op: &UpdateOp) -> Result<String>;  // returns executed SQL
    async fn insert_row(&self, op: &InsertOp) -> Result<String>;  // returns executed SQL
    async fn delete_rows(&self, op: &DeleteOp) -> Result<String>; // returns executed SQL

    async fn run_migration(&self, sql: &str) -> Result<()>;
}

/// Relationship between two tables (for schema diagram)
pub struct Relationship {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    pub constraint_name: String,
}
```

### Driver registry

```rust
// src-tauri/src/commands/db/drivers/mod.rs
pub fn all_drivers() -> Vec<Box<dyn DbDriver>> {
    vec![
        Box::new(PostgresDriver),
        Box::new(MysqlDriver),
        Box::new(SqliteDriver),
        Box::new(MongoDriver),
        // future: Box::new(ClickhouseDriver), Box::new(BigQueryDriver), ...
    ]
}

pub fn get_driver(config_type: &str) -> Option<&'static dyn DbDriver> { ... }
```

Each driver lives in its own file: `drivers/postgres.rs`, `drivers/mysql.rs`, etc.

### SQL generation helpers

`update_row`, `insert_row`, `delete_rows` on the trait return the SQL string that will be
executed. The Tauri command layer returns this SQL to the frontend so the confirmation modal
can show it verbatim before the user confirms.

---

## Features

### Sidebar — Connection & Schema Browser

Inspired by the Git tab's branch/diff layout: a **connection selector dropdown** at the top,
and a **lazy tree** below that drills from database → schema → table → sub-nodes.

```
┌─────────────────────────────────┐
│  [▼ local-postgres  ●]          │  ← connection dropdown (status dot)
├─────────────────────────────────┤
│  ▾ myapp_db                     │  ← database
│    ▾ public                     │  ← schema
│      ▾ users            [table] │
│          id  bigserial  PK      │
│          email  varchar  UNIQUE │
│          created_at  timestamptz│
│        ▸ Indexes (2)            │
│        ▸ Triggers (1)           │
│      ▾ orders           [table] │
│          ...                    │
│      ▸ Views (3)                │
│      ▸ Functions (5)            │
│  ▸ analytics_db                 │
└─────────────────────────────────┘
```

**MongoDB** adapts automatically — `TreeStructure` has no "schema" level:

```
[▼ atlas-dev  ●]
  ▾ myapp          ← database
    ▾ users        ← collection (same UI component as "table")
        _id  ObjectId
        email  string
    ▾ orders
        ...
```

**Nodes and actions:**
- Click **connection** → connect/disconnect toggle (if not yet connected, prompt for missing env vars).
- Click **database/schema** → opens Schema Diagram tab (flow chart).
- Click **table/collection** → opens Data Tab for that table.
- Click **column** → highlights column in any open Data Tab for that table.
- Right-click table → New Query, Copy name, Show DDL.
- Right-click connection → Edit config, Test connection, Duplicate, Delete.

**Lazy loading:** tree nodes load their children on first expand and cache them. A refresh
button at the top of the sidebar clears the cache and re-fetches.

---

### Schema Diagram Tab

Opened when the user clicks a **database** or **schema** node in the sidebar.

- Rendered using **[Svelte Flow](https://svelteflow.dev)** (also known as `@xyflow/svelte`).
  Evaluate `svelvet` as an alternative but prefer Svelte Flow for maturity and active maintenance.
- Each table is a node card showing: table name, list of columns (name + type + key badges).
- FK relationships rendered as edges with arrow tips. Source column → target column.
- Layout: auto-dagre on first open; user can drag nodes; layout persists in
  `.anide/database/<connection-name>/.diagram-layout.json` (gitignored).
- Controls: zoom, fit-to-view, toggle "show all columns" vs "PK + FK only" mode.
- Click a node → opens that table's Data Tab.
- For **MongoDB** collections: no FK edges (unless the user annotates them manually in future);
  just show the inferred schema from a sample of documents.

---

### Data Tab (table/collection viewer)

Opened by clicking a table in the sidebar or the schema diagram.

#### Table component

Use **[svelte-tablecn](https://github.com/itisyb/svelte-tablecn)** as the base table
component. It is built on TanStack Table + shadcn patterns and provides:
- Column sorting, filtering, resizing
- Row selection (checkboxes)
- Virtualized rendering — only visible rows are rendered in the DOM
- Accessible keyboard navigation

Wrap it in an infinite-scroll container: load 100 rows at a time, fetch the next page when
the user scrolls within 200px of the bottom (using an IntersectionObserver sentinel row).

#### Row count

A **"Count" button** in the toolbar (not automatic). Clicking it runs
`SELECT COUNT(*) FROM table` (or equivalent) and shows the result inline next to the button.
This matches DBeaver's explicit count behaviour — it avoids expensive full-table scans on
large tables unless the user requests it.

#### Toolbar

```
[connection > db > schema > table]   ← breadcrumb
[Count: —]  [Filter ▼]  [Add Row]  [Export ▼]  [Refresh]
```

#### Inline editing

- Double-click a cell → turns into an editable input.
- On `Enter` or blur: opens a **confirmation modal** showing:
  - The SQL that will be executed (e.g. `UPDATE users SET email = 'new@example.com' WHERE id = 42`).
  - Old value vs. new value.
  - "Execute" and "Cancel" buttons.
- Executes via `db_update_row`; refreshes only the affected row on success.

#### Row deletion

- Select rows with checkboxes → Delete button appears in toolbar.
- **Confirmation modal** shows:
  - The SQL: `DELETE FROM users WHERE id IN (42, 43, 44)`.
  - Count of rows affected.
  - Destructive red "Delete N rows" button + Cancel.

#### Add row

- "Add Row" button → opens a modal with a form (one field per column, types respected).
- Shows the `INSERT INTO ...` SQL in a preview section before confirming.
- On success, row appears at top of the table.

#### NULL display

`NULL` cells styled distinctly from empty strings (dimmed italic `NULL` badge).

#### Export

"Export ▼" dropdown: Export CSV, Export JSON (current page or all rows with a separate
"Export all" option that fetches remaining pages).

---

### Query Editor Tab

- CodeMirror 6 with per-dialect SQL support (postgres, mysql, sqlite dialects).
- `Ctrl+Enter` runs entire editor or selected text.
- Results shown in the same **svelte-tablecn** table (same virtualization, same export).
- "Explain" button (`Ctrl+Shift+Enter`): runs `EXPLAIN ANALYZE` (or dialect equivalent)
  and shows the plan as formatted text.
- Multiple query tabs per connection, named "Query 1", "Query 2" etc.

> History (last N queries per connection) is deferred. Do not implement now.

---

### Connection Manager

- Managed via the sidebar dropdown and the connections list (Settings panel or sidebar gear icon).
- Create / edit / delete connections via a modal form.
- "Test Connection" resolves templates against current env context, pings DB, shows latency
  and server version.
- Connection state (connected / disconnected / error) shown as a coloured dot.

### Migration Tracker

- Scans project for SQL files matching a configurable glob (default `migrations/*.sql`).
- Tracks applied migrations in a `_anide_migrations` table created on first use.
- UI: list with status (applied / pending), applied-at timestamp.
- Run: executes selected pending migration + records it.
- Rollback: not automatic — shows a warning to write a rollback manually.

---

## Tauri commands (Rust)

### State

```rust
pub struct DbState {
    // keyed by connection_name
    connections: Mutex<HashMap<String, Box<dyn DbConnection>>>,
}
```

### Commands

```rust
// Connection management
db_list_connections(project_path: String) -> Result<Vec<ConnectionMeta>>
db_save_connection(project_path: String, conn: ConnectionData) -> Result<()>
db_delete_connection(project_path: String, name: String) -> Result<()>
db_test_connection(project_path: String, conn: ConnectionData, env_context: EnvContext)
  -> Result<TestResult>   // { latency_ms, server_version }
db_connect(project_path: String, connection_name: String, env_context: EnvContext)
  -> Result<()>
db_disconnect(project_path: String, connection_name: String) -> Result<()>

// Tree / schema inspection
db_get_tree_structure(project_path: String, connection_name: String)
  -> Result<TreeStructure>  // driver-specific shape; used to configure sidebar
db_list_databases(project_path: String, connection_name: String)
  -> Result<Vec<String>>
db_list_schemas(project_path: String, connection_name: String, database: String)
  -> Result<Vec<String>>
db_list_tables(project_path: String, connection_name: String, database: String, schema: String)
  -> Result<Vec<TableMeta>>
db_list_columns(project_path: String, connection_name: String, database: String, schema: String, table: String)
  -> Result<Vec<ColumnMeta>>
db_list_indexes(project_path: String, connection_name: String, database: String, schema: String, table: String)
  -> Result<Vec<IndexMeta>>
db_list_views(project_path: String, connection_name: String, database: String, schema: String)
  -> Result<Vec<String>>
db_list_functions(project_path: String, connection_name: String, database: String, schema: String)
  -> Result<Vec<FunctionMeta>>
db_get_relationships(project_path: String, connection_name: String, database: String, schema: String)
  -> Result<Vec<Relationship>>  // for schema diagram edges

// Data
db_query_page(project_path: String, connection_name: String, sql: String, offset: u64, limit: u32)
  -> Result<QueryResult>   // { columns, rows, duration_ms }
db_count_table(project_path: String, connection_name: String, database: String, schema: String, table: String)
  -> Result<u64>

// Write operations — all return the SQL that was (or will be) executed
db_update_row(project_path: String, connection_name: String, op: UpdateOp)
  -> Result<ExecutedOp>   // { sql: String, affected: u64 }
db_insert_row(project_path: String, connection_name: String, op: InsertOp)
  -> Result<ExecutedOp>
db_delete_rows(project_path: String, connection_name: String, op: DeleteOp)
  -> Result<ExecutedOp>

// Preview SQL without executing (for the confirmation modal dry-run)
db_preview_update(op: UpdateOp) -> Result<String>  // pure SQL string, no DB call
db_preview_insert(op: InsertOp) -> Result<String>
db_preview_delete(op: DeleteOp) -> Result<String>

// Queries (saved)
db_list_queries(project_path: String, connection_name: String) -> Result<Vec<QueryMeta>>
db_save_query(project_path: String, connection_name: String, query: QueryData) -> Result<()>
db_delete_query(project_path: String, connection_name: String, name: String) -> Result<()>

// Migrations
db_list_migrations(project_path: String, connection_name: String) -> Result<Vec<MigrationStatus>>
db_run_migration(project_path: String, connection_name: String, file_path: String) -> Result<()>
```

### Rust deps to add

```toml
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "mysql", "sqlite", "json", "chrono", "uuid"] }
mongodb = { version = "3", features = ["tokio-runtime"] }
async-trait = "0.1"
```

---

## Frontend (Svelte)

### New files

```
src/lib/components/workspace/DbQueryTab.svelte      # query editor + results
src/lib/components/workspace/DbDataTab.svelte       # table data viewer (svelte-tablecn)
src/lib/components/workspace/DbDiagramTab.svelte    # schema flow diagram (Svelte Flow)
src/lib/components/panels/DbPanel.svelte            # connection dropdown + schema tree
src/lib/components/db/DbTable.svelte                # shared svelte-tablecn wrapper
src/lib/components/db/SqlConfirmModal.svelte        # shows SQL before executing write ops
src/lib/commands/db.js                              # Tauri command wrappers
```

### DbPanel.svelte (sidebar)

- **Connection dropdown** at top: lists all connections with status dots.
  Selecting a different connection connects it if not already connected.
- **Schema tree** below: lazy-loads children on first expand.
  Tree shape driven by `db_get_tree_structure` — the frontend has no hardcoded notion of
  "schema" vs "collection"; it reads `TreeStructure.levels` from Rust.
- Click handlers described in the **Sidebar** section above.

### DbTable.svelte (shared table component)

Wraps `svelte-tablecn` with:
- IntersectionObserver-based infinite scroll (load next 100 rows)
- NULL cell rendering (dimmed italic badge)
- Double-click-to-edit cell (triggers `SqlConfirmModal` flow)
- Checkbox row selection
- Export toolbar button

### SqlConfirmModal.svelte

A reusable modal used for all write operations (update, insert, delete). Props:

```ts
{
  title: string,       // "Confirm Update" / "Confirm Delete" / "Confirm Insert"
  sql: string,         // the SQL that will execute — shown in a syntax-highlighted block
  summary?: string,    // e.g. "Updating 1 row in users"
  destructive?: bool,  // if true, confirm button is red
  onConfirm: () => Promise<void>,
  onCancel: () => void,
}
```

The SQL preview uses a read-only CodeMirror block so it's syntax-highlighted and selectable.
This lets users see exactly what Anide is doing — and learn from it.

### DbDiagramTab.svelte (schema diagram)

- Uses `@xyflow/svelte` (Svelte Flow).
- Each table = a custom node component (`DbTableNode.svelte`): table name header + scrollable
  column list (name, type, key badges).
- FK edges drawn from source column → target column with animated arrows.
- Toggle: "PK + FK columns only" / "all columns".
- Auto-dagre layout on first load; layout saved to `.anide/database/<conn>/.diagram-layout.json`.
- Click node → opens DbDataTab for that table.
- MongoDB: nodes show inferred field types from a sample; no FK edges (not applicable).

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

Frontend renders each type with appropriate formatting (dates localised, bytes shown with size
suffix, JSON pretty-printed on expand).
