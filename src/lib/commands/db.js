import { invoke } from "@tauri-apps/api/core";

// ── Connection management ─────────────────────────────────────────────────────

export function dbListConnections(projectPath) {
  return invoke("db_list_connections", { projectPath });
}

export function dbSaveConnection(projectPath, config) {
  return invoke("db_save_connection", { projectPath, config });
}

export function dbDeleteConnection(projectPath, name) {
  return invoke("db_delete_connection", { projectPath, name });
}

export function dbTestConnection(projectPath, name, envContext) {
  return invoke("db_test_connection", { projectPath, name, envContext });
}

export function dbConnect(projectPath, name, envContext) {
  return invoke("db_connect", { projectPath, name, envContext });
}

export function dbDisconnect(name) {
  return invoke("db_disconnect", { name });
}

// ── Tree / schema ─────────────────────────────────────────────────────────────

export function dbGetTreeStructure(projectPath, name) {
  return invoke("db_get_tree_structure", { projectPath, name });
}

export function dbListDatabases(name) {
  return invoke("db_list_databases", { name });
}

export function dbListSchemas(name, database) {
  return invoke("db_list_schemas", { name, database });
}

export function dbListTables(name, database, schema) {
  return invoke("db_list_tables", { name, database, schema });
}

export function dbListColumns(name, database, schema, table) {
  return invoke("db_list_columns", { name, database, schema, table });
}

export function dbListIndexes(name, database, schema, table) {
  return invoke("db_list_indexes", { name, database, schema, table });
}

export function dbListViews(name, database, schema) {
  return invoke("db_list_views", { name, database, schema });
}

export function dbListFunctions(name, database, schema) {
  return invoke("db_list_functions", { name, database, schema });
}

export function dbGetRelationships(name, database, schema) {
  return invoke("db_get_relationships", { name, database, schema });
}

// ── Data ──────────────────────────────────────────────────────────────────────

export function dbQueryPage(name, sql, offset, limit) {
  return invoke("db_query_page", { name, sql, offset, limit });
}

export function dbCountTable(name, database, schema, table) {
  return invoke("db_count_table", { name, database, schema, table });
}

// ── Write ops ─────────────────────────────────────────────────────────────────

export function dbUpdateRow(name, op) {
  return invoke("db_update_row", { name, op });
}

export function dbInsertRow(name, op) {
  return invoke("db_insert_row", { name, op });
}

export function dbDeleteRows(name, op) {
  return invoke("db_delete_rows", { name, op });
}

export function dbPreviewUpdate(op) {
  return invoke("db_preview_update", { op });
}

export function dbPreviewInsert(op) {
  return invoke("db_preview_insert", { op });
}

export function dbPreviewDelete(op) {
  return invoke("db_preview_delete", { op });
}

// ── Saved queries ─────────────────────────────────────────────────────────────

export function dbListQueries(projectPath, name) {
  return invoke("db_list_queries", { projectPath, name });
}

export function dbSaveQuery(projectPath, name, query) {
  return invoke("db_save_query", { projectPath, name, query });
}

export function dbDeleteQuery(projectPath, connName, fileName, collection = null) {
  return invoke("db_delete_query", { projectPath, connName, fileName, collection });
}

export function dbCreateQueryCollection(projectPath, connName, collectionName, parentPath = null) {
  return invoke("db_create_query_collection", { projectPath, connName, collectionName, parentPath });
}

export function dbRenameQuery(projectPath, connName, fileName, collection, newName) {
  return invoke("db_rename_query", { projectPath, connName, fileName, collection, newName });
}

export function dbDuplicateQuery(projectPath, connName, fileName, collection = null) {
  return invoke("db_duplicate_query", { projectPath, connName, fileName, collection });
}

export function dbDeleteQueryCollection(projectPath, connName, path) {
  return invoke("db_delete_query_collection", { projectPath, connName, path });
}

export function dbDuplicateQueryCollection(projectPath, connName, path) {
  return invoke("db_duplicate_query_collection", { projectPath, connName, path });
}

export function dbRenameQueryCollection(projectPath, connName, path, newName) {
  return invoke("db_rename_query_collection", { projectPath, connName, path, newName });
}
