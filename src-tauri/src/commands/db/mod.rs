// @ts-nocheck
pub mod drivers;

use crate::error::AppError;
use crate::utils::frontmatter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ── Shared types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionConfig {
    pub name: String,
    #[serde(rename = "type", alias = "conn_type", default)]
    pub conn_type: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default, deserialize_with = "de_as_string")]
    pub port: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub connection_string: Option<String>,
    #[serde(skip)]
    pub notes: String,
    #[serde(skip, default)]
    pub project_path: String,
}

fn de_as_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let v = Option::<serde_yaml::Value>::deserialize(d)?;
    Ok(v.map(|val| match val {
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::String(s) => s,
        other => format!("{other:?}"),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMeta {
    pub name: String,
    pub conn_type: String,
    pub connected: bool,
    pub host: Option<String>,
    pub database: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub latency_ms: u64,
    pub server_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeStructure {
    pub levels: Vec<TreeLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeLevel {
    pub label: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMeta {
    pub name: String,
    pub table_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub name: String,
    pub col_type: String,
    pub nullable: bool,
    pub default_val: Option<String>,
    pub is_primary: bool,
    pub is_unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMeta {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMeta {
    pub name: String,
    pub return_type: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    pub constraint_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOp {
    pub database: String,
    pub schema: String,
    pub table: String,
    pub pk_column: String,
    pub pk_value: serde_json::Value,
    pub column: String,
    pub new_value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertOp {
    pub database: String,
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
    pub values: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteOp {
    pub database: String,
    pub schema: String,
    pub table: String,
    pub pk_column: String,
    pub pk_values: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedOp {
    pub sql: String,
    pub affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMeta {
    pub name: String,
    pub description: String,
    pub file_name: String,
    pub sql: String,
    pub collection: Option<String>, // dir_name of collection, or None for root
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMeta {
    pub name: String,
    pub dir_name: String,
    pub path: String,                    // full path relative to queries root
    pub queries: Vec<QueryMeta>,
    pub collections: Vec<CollectionMeta>, // nested collections
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueriesTree {
    pub root: Vec<QueryMeta>,
    pub collections: Vec<CollectionMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryData {
    pub name: String,
    pub description: String,
    pub sql: String,
    pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvContext {
    pub vars: HashMap<String, String>,
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct DbState {
    pub connections: Mutex<HashMap<String, Arc<dyn drivers::DbConnection>>>,
}

impl DbState {
    pub fn new() -> Self {
        Self { connections: Mutex::new(HashMap::new()) }
    }
}

// ── Path helpers ──────────────────────────────────────────────────────────────

fn db_root(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".anide").join("database")
}

fn conn_dir(project_path: &str, name: &str) -> PathBuf {
    db_root(project_path).join(name)
}

fn conn_config_path(project_path: &str, name: &str) -> PathBuf {
    conn_dir(project_path, name).join("config.md")
}

fn queries_dir(project_path: &str, name: &str) -> PathBuf {
    conn_dir(project_path, name).join("queries")
}

// ── Template resolver ─────────────────────────────────────────────────────────

pub fn resolve_conn_template(template: &str, env_vars: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        result.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        if let Some(close) = after.find("}}") {
            let token = after[..close].trim();
            let resolved = if let Some(var_name) = token.strip_prefix("env.") {
                env_vars.get(var_name).map(String::as_str).unwrap_or(token)
            } else {
                env_vars.get(token).map(String::as_str).unwrap_or(token)
            };
            result.push_str(resolved);
            rest = &after[close + 2..];
        } else {
            result.push_str("{{");
            rest = after;
        }
    }
    result.push_str(rest);
    result
}

// ── SQL preview helpers ────────────────────────────────────────────────────────

pub fn json_to_sql_literal(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => if *b { "TRUE".to_string() } else { "FALSE".to_string() },
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        serde_json::Value::Array(a) => format!("'{}'", serde_json::to_string(a).unwrap_or_default().replace('\'', "''")),
        serde_json::Value::Object(o) => format!("'{}'", serde_json::to_string(o).unwrap_or_default().replace('\'', "''")),
    }
}

pub fn preview_update_sql(op: &UpdateOp) -> String {
    let schema_prefix = if op.schema.is_empty() { String::new() } else { format!("\"{}\".", op.schema) };
    format!(
        "UPDATE {}\"{}\"\nSET \"{}\" = {}\nWHERE \"{}\" = {};",
        schema_prefix, op.table, op.column,
        json_to_sql_literal(&op.new_value),
        op.pk_column, json_to_sql_literal(&op.pk_value)
    )
}

pub fn preview_insert_sql(op: &InsertOp) -> String {
    let schema_prefix = if op.schema.is_empty() { String::new() } else { format!("\"{}\".", op.schema) };
    let cols: Vec<String> = op.columns.iter().map(|c| format!("\"{}\"", c)).collect();
    let vals: Vec<String> = op.values.iter().map(json_to_sql_literal).collect();
    format!(
        "INSERT INTO {}\"{}\" ({})\nVALUES ({});",
        schema_prefix, op.table, cols.join(", "), vals.join(", ")
    )
}

pub fn preview_delete_sql(op: &DeleteOp) -> String {
    let schema_prefix = if op.schema.is_empty() { String::new() } else { format!("\"{}\".", op.schema) };
    let values: Vec<String> = op.pk_values.iter().map(json_to_sql_literal).collect();
    format!(
        "DELETE FROM {}\"{}\"\nWHERE \"{}\" IN ({});",
        schema_prefix, op.table, op.pk_column, values.join(", ")
    )
}

// ── File I/O ──────────────────────────────────────────────────────────────────

pub fn read_connection_config(project_path: &str, name: &str) -> Result<ConnectionConfig, AppError> {
    let path = conn_config_path(project_path, name);
    let content = std::fs::read_to_string(&path)
        .map_err(|_| AppError::NotFound(format!("Connection config not found: {name}")))?;
    let (yaml_str, body) = frontmatter::parse(&content)?;
    let mut config: ConnectionConfig = serde_yaml::from_str(&yaml_str)?;
    config.notes = body;
    Ok(config)
}

pub fn write_connection_config(project_path: &str, config: &ConnectionConfig) -> Result<(), AppError> {
    let dir = conn_dir(project_path, &config.name);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.md");
    // Serialize without the notes field (it goes in the body)
    let mut map = serde_yaml::Mapping::new();
    map.insert(serde_yaml::Value::String("name".into()), serde_yaml::Value::String(config.name.clone()));
    map.insert(serde_yaml::Value::String("type".into()), serde_yaml::Value::String(config.conn_type.clone()));
    if let Some(v) = &config.host { map.insert("host".into(), v.clone().into()); }
    if let Some(v) = &config.port { map.insert("port".into(), v.clone().into()); }
    if let Some(v) = &config.database { map.insert("database".into(), v.clone().into()); }
    if let Some(v) = &config.username { map.insert("username".into(), v.clone().into()); }
    if let Some(v) = &config.password { map.insert("password".into(), v.clone().into()); }
    if config.ssl { map.insert("ssl".into(), true.into()); }
    if let Some(v) = &config.path { map.insert("path".into(), v.clone().into()); }
    if let Some(v) = &config.connection_string { map.insert("connection_string".into(), v.clone().into()); }
    let yaml_str = serde_yaml::to_string(&serde_yaml::Value::Mapping(map))?;
    let content = frontmatter::serialize(&yaml_str, &config.notes);
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn list_connections(project_path: &str) -> Result<Vec<String>, AppError> {
    let root = db_root(project_path);
    if !root.exists() { return Ok(Vec::new()); }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let config_path = entry.path().join("config.md");
            if config_path.exists() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn db_list_connections(
    project_path: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<ConnectionMeta>, AppError> {
    let names = list_connections(&project_path)?;
    let conns = state.connections.lock().unwrap();
    let metas = names.iter().map(|name| {
        let cfg = read_connection_config(&project_path, name).unwrap_or_default();
        ConnectionMeta {
            name: name.clone(),
            conn_type: cfg.conn_type,
            connected: conns.contains_key(name),
            host: cfg.host,
            database: cfg.database,
        }
    }).collect();
    Ok(metas)
}

#[tauri::command]
pub fn db_save_connection(
    project_path: String,
    config: ConnectionConfig,
) -> Result<(), AppError> {
    write_connection_config(&project_path, &config)
}

#[tauri::command]
pub fn db_delete_connection(
    project_path: String,
    name: String,
    state: tauri::State<'_, DbState>,
) -> Result<(), AppError> {
    // Disconnect first if connected
    state.connections.lock().unwrap().remove(&name);
    let dir = conn_dir(&project_path, &name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn db_test_connection(
    project_path: String,
    name: String,
    env_context: EnvContext,
) -> Result<TestResult, AppError> {
    let mut config = read_connection_config(&project_path, &name)?;
    config.project_path = project_path;
    let driver = drivers::get_driver_for(&config.conn_type)
        .ok_or_else(|| AppError::Db(format!("Unknown driver: {}", config.conn_type)))?;
    driver.test_connection(&config, &env_context.vars).await
}

#[tauri::command]
pub async fn db_connect(
    project_path: String,
    name: String,
    env_context: EnvContext,
    state: tauri::State<'_, DbState>,
) -> Result<(), AppError> {
    let mut config = read_connection_config(&project_path, &name)?;
    config.project_path = project_path;
    let driver = drivers::get_driver_for(&config.conn_type)
        .ok_or_else(|| AppError::Db(format!("Unknown driver: {}", config.conn_type)))?;
    let conn = driver.connect(&config, &env_context.vars).await?;
    state.connections.lock().unwrap().insert(name, Arc::from(conn));
    Ok(())
}

#[tauri::command]
pub fn db_disconnect(
    name: String,
    state: tauri::State<'_, DbState>,
) -> Result<(), AppError> {
    state.connections.lock().unwrap().remove(&name);
    Ok(())
}

#[tauri::command]
pub fn db_get_tree_structure(
    project_path: String,
    name: String,
) -> Result<TreeStructure, AppError> {
    let config = read_connection_config(&project_path, &name)?;
    let driver = drivers::get_driver_for(&config.conn_type)
        .ok_or_else(|| AppError::Db(format!("Unknown driver: {}", config.conn_type)))?;
    Ok(driver.tree_structure())
}

fn get_conn(name: &str, state: &tauri::State<'_, DbState>) -> Result<Arc<dyn drivers::DbConnection>, AppError> {
    state.connections.lock().unwrap()
        .get(name)
        .cloned()
        .ok_or_else(|| AppError::Db(format!("Not connected: {name}")))
}

#[tauri::command]
pub async fn db_list_databases(
    name: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<String>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.list_databases().await
}

#[tauri::command]
pub async fn db_list_schemas(
    name: String,
    database: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<String>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.list_schemas(&database).await
}

#[tauri::command]
pub async fn db_list_tables(
    name: String,
    database: String,
    schema: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<TableMeta>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.list_tables(&database, &schema).await
}

#[tauri::command]
pub async fn db_list_columns(
    name: String,
    database: String,
    schema: String,
    table: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<ColumnMeta>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.list_columns(&database, &schema, &table).await
}

#[tauri::command]
pub async fn db_list_indexes(
    name: String,
    database: String,
    schema: String,
    table: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<IndexMeta>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.list_indexes(&database, &schema, &table).await
}

#[tauri::command]
pub async fn db_list_views(
    name: String,
    database: String,
    schema: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<String>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.list_views(&database, &schema).await
}

#[tauri::command]
pub async fn db_list_functions(
    name: String,
    database: String,
    schema: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<FunctionMeta>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.list_functions(&database, &schema).await
}

#[tauri::command]
pub async fn db_get_relationships(
    name: String,
    database: String,
    schema: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<Relationship>, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.get_relationships(&database, &schema).await
}

#[tauri::command]
pub async fn db_query_page(
    name: String,
    sql: String,
    offset: u64,
    limit: u32,
    state: tauri::State<'_, DbState>,
) -> Result<QueryResult, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.query_page(&sql, offset, limit).await
}

#[tauri::command]
pub async fn db_count_table(
    name: String,
    database: String,
    schema: String,
    table: String,
    state: tauri::State<'_, DbState>,
) -> Result<u64, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.count_table(&database, &schema, &table).await
}

#[tauri::command]
pub async fn db_update_row(
    name: String,
    op: UpdateOp,
    state: tauri::State<'_, DbState>,
) -> Result<ExecutedOp, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.execute_update(&op).await
}

#[tauri::command]
pub async fn db_insert_row(
    name: String,
    op: InsertOp,
    state: tauri::State<'_, DbState>,
) -> Result<ExecutedOp, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.execute_insert(&op).await
}

#[tauri::command]
pub async fn db_delete_rows(
    name: String,
    op: DeleteOp,
    state: tauri::State<'_, DbState>,
) -> Result<ExecutedOp, AppError> {
    let conn = get_conn(&name, &state)?;
    conn.execute_delete(&op).await
}

#[tauri::command]
pub fn db_preview_update(op: UpdateOp) -> Result<String, AppError> {
    Ok(preview_update_sql(&op))
}

#[tauri::command]
pub fn db_preview_insert(op: InsertOp) -> Result<String, AppError> {
    Ok(preview_insert_sql(&op))
}

#[tauri::command]
pub fn db_preview_delete(op: DeleteOp) -> Result<String, AppError> {
    Ok(preview_delete_sql(&op))
}

fn read_query_meta(path: &std::path::Path, collection: Option<String>) -> Option<QueryMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    let (yaml_str, sql) = frontmatter::parse(&content).ok()?;
    #[derive(Deserialize, Default)]
    struct QFm { #[serde(default)] name: String, #[serde(default)] description: String }
    let fm: QFm = serde_yaml::from_str(&yaml_str).unwrap_or_default();
    let file_name = path.file_name()?.to_str()?.to_string();
    let display_name = if fm.name.is_empty() {
        path.file_stem()?.to_str()?.to_string()
    } else { fm.name };
    Some(QueryMeta { name: display_name, description: fm.description, file_name, sql, collection })
}

fn scan_queries_dir(dir: &Path, path_prefix: &str) -> (Vec<QueryMeta>, Vec<CollectionMeta>) {
    let mut queries = Vec::new();
    let mut collections = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return (queries, collections); };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let dir_name = p.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
            let col_path = if path_prefix.is_empty() {
                dir_name.clone()
            } else {
                format!("{}/{}", path_prefix, dir_name)
            };
            let display_name = dir_name.replace('-', " ");
            let (sub_queries, sub_collections) = scan_queries_dir(&p, &col_path);
            collections.push(CollectionMeta {
                name: display_name,
                dir_name: dir_name.clone(),
                path: col_path,
                queries: sub_queries,
                collections: sub_collections,
            });
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let collection = if path_prefix.is_empty() { None } else { Some(path_prefix.to_string()) };
            if let Some(q) = read_query_meta(&p, collection) { queries.push(q); }
        }
    }
    queries.sort_by(|a, b| a.name.cmp(&b.name));
    collections.sort_by(|a, b| a.name.cmp(&b.name));
    (queries, collections)
}

#[tauri::command]
pub fn db_list_queries(project_path: String, name: String) -> Result<QueriesTree, AppError> {
    let dir = queries_dir(&project_path, &name);
    if !dir.exists() { return Ok(QueriesTree { root: Vec::new(), collections: Vec::new() }); }
    let (root, collections) = scan_queries_dir(&dir, "");
    Ok(QueriesTree { root, collections })
}

fn join_collection_path(base: PathBuf, collection: &str) -> PathBuf {
    // Walk components explicitly so forward-slash paths work correctly on Windows.
    collection.split('/').fold(base, |acc, seg| if seg.is_empty() { acc } else { acc.join(seg) })
}

#[tauri::command]
pub fn db_save_query(project_path: String, name: String, query: QueryData) -> Result<String, AppError> {
    let base = queries_dir(&project_path, &name);
    let dir = match &query.collection {
        Some(c) => { let d = join_collection_path(base, c); std::fs::create_dir_all(&d)?; d }
        None => { std::fs::create_dir_all(&base)?; base }
    };
    let file_name = query.name.to_lowercase().replace(' ', "-") + ".md";
    let yaml = format!("name: {:?}\ndescription: {:?}\n", query.name, query.description);
    let content = frontmatter::serialize(&yaml, &query.sql);
    std::fs::write(dir.join(&file_name), content)?;
    Ok(file_name)
}

#[tauri::command]
pub fn db_delete_query(project_path: String, conn_name: String, file_name: String, collection: Option<String>) -> Result<(), AppError> {
    let base = queries_dir(&project_path, &conn_name);
    let path = match collection {
        Some(c) => join_collection_path(base, &c).join(&file_name),
        None => base.join(&file_name),
    };
    if path.exists() { std::fs::remove_file(&path)?; }
    Ok(())
}

#[tauri::command]
pub fn db_create_query_collection(
    project_path: String,
    conn_name: String,
    collection_name: String,
    parent_path: Option<String>,
) -> Result<String, AppError> {
    let dir_name = collection_name.to_lowercase().replace(' ', "-");
    let base = queries_dir(&project_path, &conn_name);
    let parent_dir = match &parent_path { Some(p) => join_collection_path(base, p), None => base };
    let new_path = match &parent_path {
        Some(p) => format!("{}/{}", p, dir_name),
        None => dir_name.clone(),
    };
    std::fs::create_dir_all(parent_dir.join(&dir_name))?;
    Ok(new_path)
}

#[tauri::command]
pub fn db_rename_query(
    project_path: String, conn_name: String, file_name: String,
    collection: Option<String>, new_name: String,
) -> Result<String, AppError> {
    let base = queries_dir(&project_path, &conn_name);
    let dir = match &collection { Some(c) => join_collection_path(base, c), None => base };
    let old_path = dir.join(&file_name);
    let new_file_name = new_name.to_lowercase().replace(' ', "-") + ".md";
    let new_path = dir.join(&new_file_name);
    let content = std::fs::read_to_string(&old_path)
        .map_err(|_| AppError::NotFound(format!("Query not found: {file_name}")))?;
    let (yaml_str, sql) = frontmatter::parse(&content)?;
    let mut map: serde_yaml::Mapping = serde_yaml::from_str(&yaml_str).unwrap_or_default();
    map.insert("name".into(), serde_yaml::Value::String(new_name));
    let new_yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(map))?;
    std::fs::write(&new_path, frontmatter::serialize(&new_yaml, &sql))?;
    if old_path != new_path { std::fs::remove_file(&old_path)?; }
    Ok(new_file_name)
}

#[tauri::command]
pub fn db_duplicate_query(
    project_path: String, conn_name: String, file_name: String, collection: Option<String>,
) -> Result<(), AppError> {
    let base = queries_dir(&project_path, &conn_name);
    let dir = match &collection { Some(c) => join_collection_path(base, c), None => base };
    let path = dir.join(&file_name);
    let content = std::fs::read_to_string(&path)
        .map_err(|_| AppError::NotFound(format!("Query not found: {file_name}")))?;
    let (yaml_str, sql) = frontmatter::parse(&content)?;
    #[derive(Deserialize, Default)]
    struct QFm { #[serde(default)] name: String, #[serde(default)] description: String }
    let fm: QFm = serde_yaml::from_str(&yaml_str).unwrap_or_default();
    let new_name = format!("Copy of {}", fm.name);
    let new_file_name = new_name.to_lowercase().replace(' ', "-") + ".md";
    let new_yaml = format!("name: {:?}\ndescription: {:?}\n", new_name, fm.description);
    std::fs::write(dir.join(&new_file_name), frontmatter::serialize(&new_yaml, &sql))?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let p = entry.path();
        let name = entry.file_name();
        if p.is_dir() { copy_dir_recursive(&p, &dst.join(&name))?; }
        else { std::fs::copy(&p, dst.join(&name))?; }
    }
    Ok(())
}

#[tauri::command]
pub fn db_delete_query_collection(project_path: String, conn_name: String, path: String) -> Result<(), AppError> {
    let dir = join_collection_path(queries_dir(&project_path, &conn_name), &path);
    if dir.exists() { std::fs::remove_dir_all(&dir)?; }
    Ok(())
}

#[tauri::command]
pub fn db_duplicate_query_collection(project_path: String, conn_name: String, path: String) -> Result<String, AppError> {
    let base = queries_dir(&project_path, &conn_name);
    let src = join_collection_path(base.clone(), &path);
    let dir_name = std::path::Path::new(&path).file_name()
        .and_then(|n| n.to_str()).unwrap_or(&path).to_string();
    let parent = std::path::Path::new(&path).parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().to_string());
    let new_dir_name = format!("copy-of-{}", dir_name);
    let new_path = match &parent {
        Some(p) => format!("{}/{}", p, new_dir_name),
        None => new_dir_name,
    };
    copy_dir_recursive(&src, &base.join(&new_path))?;
    Ok(new_path)
}

#[tauri::command]
pub fn db_rename_query_collection(
    project_path: String, conn_name: String, path: String, new_name: String,
) -> Result<String, AppError> {
    let base = queries_dir(&project_path, &conn_name);
    let old_dir = join_collection_path(base.clone(), &path);
    let new_dir_name = new_name.to_lowercase().replace(' ', "-");
    let parent = std::path::Path::new(&path).parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().to_string());
    let new_path = match &parent {
        Some(p) => format!("{}/{}", p, new_dir_name),
        None => new_dir_name,
    };
    let new_dir = base.join(&new_path);
    if old_dir != new_dir { std::fs::rename(&old_dir, &new_dir)?; }
    Ok(new_path)
}
