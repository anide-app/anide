use super::{DbConnection, DbDriver};
use crate::commands::db::*;
use crate::error::AppError;
use async_trait::async_trait;
use sqlx::{Column, Pool, Row, TypeInfo};
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use std::collections::HashMap;
use std::time::Instant;

pub struct PostgresDriver;

#[async_trait]
impl DbDriver for PostgresDriver {
    fn display_name(&self) -> &'static str { "PostgreSQL" }
    fn config_type(&self) -> &'static str { "postgresql" }

    fn tree_structure(&self) -> TreeStructure {
        TreeStructure {
            levels: vec![
                TreeLevel { label: "Database".into(), icon: "database".into() },
                TreeLevel { label: "Schema".into(), icon: "layers".into() },
                TreeLevel { label: "Table".into(), icon: "table".into() },
            ],
        }
    }

    async fn connect(&self, config: &ConnectionConfig, env_vars: &HashMap<String, String>) -> Result<Box<dyn DbConnection>, AppError> {
        let url = pg_url(config, env_vars);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .map_err(|e| AppError::Db(format!("PostgreSQL: {e}")))?;
        Ok(Box::new(PgConn { pool }))
    }

    async fn test_connection(&self, config: &ConnectionConfig, env_vars: &HashMap<String, String>) -> Result<TestResult, AppError> {
        let url = pg_url(config, env_vars);
        let start = Instant::now();
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await
            .map_err(|e| AppError::Db(format!("PostgreSQL: {e}")))?;
        let latency_ms = start.elapsed().as_millis() as u64;
        let row = sqlx::query("SELECT version()").fetch_one(&pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        pool.close().await;
        Ok(TestResult { latency_ms, server_version: version })
    }
}

fn pg_url(config: &ConnectionConfig, env: &HashMap<String, String>) -> String {
    if let Some(cs) = &config.connection_string {
        return resolve_conn_template(cs, env);
    }
    let host = resolve_conn_template(config.host.as_deref().unwrap_or("localhost"), env);
    let port = resolve_conn_template(config.port.as_deref().unwrap_or("5432"), env);
    let db   = resolve_conn_template(config.database.as_deref().unwrap_or("postgres"), env);
    let user = resolve_conn_template(config.username.as_deref().unwrap_or("postgres"), env);
    let pass = resolve_conn_template(config.password.as_deref().unwrap_or(""), env);
    let ssl  = if config.ssl { "?sslmode=require" } else { "" };
    format!("postgresql://{}:{}@{}:{}/{}{}", urlencoding::encode(&user), urlencoding::encode(&pass), host, port, db, ssl)
}

// ── Connection ────────────────────────────────────────────────────────────────

pub struct PgConn { pool: PgPool }

#[async_trait]
impl DbConnection for PgConn {
    fn driver_type(&self) -> &'static str { "postgresql" }

    async fn list_databases(&self) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query("SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname")
            .fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect())
    }

    async fn list_schemas(&self, _database: &str) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('information_schema','pg_catalog','pg_toast','pg_temp_1','pg_toast_temp_1') \
             ORDER BY schema_name"
        ).fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect())
    }

    async fn list_tables(&self, _database: &str, schema: &str) -> Result<Vec<TableMeta>, AppError> {
        let rows = sqlx::query(
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_schema = $1 ORDER BY table_name"
        ).bind(schema).fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| TableMeta {
            name: r.try_get(0).unwrap_or_default(),
            table_type: r.try_get(1).unwrap_or_default(),
        }).collect())
    }

    async fn list_columns(&self, _database: &str, schema: &str, table: &str) -> Result<Vec<ColumnMeta>, AppError> {
        let rows = sqlx::query(
            "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default \
             FROM information_schema.columns c \
             WHERE c.table_schema = $1 AND c.table_name = $2 ORDER BY c.ordinal_position"
        ).bind(schema).bind(table).fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;

        let pk_rows = sqlx::query(
            "SELECT kcu.column_name FROM information_schema.key_column_usage kcu \
             JOIN information_schema.table_constraints tc ON tc.constraint_name = kcu.constraint_name \
             AND tc.constraint_schema = kcu.constraint_schema \
             WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = $1 AND tc.table_name = $2"
        ).bind(schema).bind(table).fetch_all(&self.pool).await.unwrap_or_default();
        let pk_set: std::collections::HashSet<String> = pk_rows.iter()
            .map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect();

        let uq_rows = sqlx::query(
            "SELECT kcu.column_name FROM information_schema.key_column_usage kcu \
             JOIN information_schema.table_constraints tc ON tc.constraint_name = kcu.constraint_name \
             AND tc.constraint_schema = kcu.constraint_schema \
             WHERE tc.constraint_type = 'UNIQUE' AND tc.table_schema = $1 AND tc.table_name = $2"
        ).bind(schema).bind(table).fetch_all(&self.pool).await.unwrap_or_default();
        let uq_set: std::collections::HashSet<String> = uq_rows.iter()
            .map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect();

        Ok(rows.iter().map(|r| {
            let name: String = r.try_get(0).unwrap_or_default();
            ColumnMeta {
                is_primary: pk_set.contains(&name),
                is_unique: uq_set.contains(&name),
                name,
                col_type: r.try_get(1).unwrap_or_default(),
                nullable: r.try_get::<String, _>(2).unwrap_or_default() == "YES",
                default_val: r.try_get::<Option<String>, _>(3).unwrap_or(None),
            }
        }).collect())
    }

    async fn list_indexes(&self, _database: &str, schema: &str, table: &str) -> Result<Vec<IndexMeta>, AppError> {
        let rows = sqlx::query(
            "SELECT i.relname, ix.indisunique, ix.indisprimary, a.attname \
             FROM pg_class t \
             JOIN pg_index ix ON t.oid = ix.indrelid \
             JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey) \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             WHERE n.nspname = $1 AND t.relname = $2"
        ).bind(schema).bind(table).fetch_all(&self.pool).await.unwrap_or_default();

        let mut map: std::collections::HashMap<String, IndexMeta> = std::collections::HashMap::new();
        for r in &rows {
            let name: String = r.try_get(0).unwrap_or_default();
            let entry = map.entry(name.clone()).or_insert(IndexMeta {
                name: name.clone(),
                is_unique: r.try_get(1).unwrap_or(false),
                is_primary: r.try_get(2).unwrap_or(false),
                columns: Vec::new(),
            });
            let col: String = r.try_get(3).unwrap_or_default();
            if !col.is_empty() { entry.columns.push(col); }
        }
        Ok(map.into_values().collect())
    }

    async fn list_views(&self, _database: &str, schema: &str) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query(
            "SELECT table_name FROM information_schema.views WHERE table_schema = $1 ORDER BY table_name"
        ).bind(schema).fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect())
    }

    async fn list_functions(&self, _database: &str, schema: &str) -> Result<Vec<FunctionMeta>, AppError> {
        let rows = sqlx::query(
            "SELECT p.proname, pg_get_function_result(p.oid), l.lanname \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             JOIN pg_language l ON l.oid = p.prolang \
             WHERE n.nspname = $1 AND p.prokind = 'f' ORDER BY p.proname"
        ).bind(schema).fetch_all(&self.pool).await.unwrap_or_default();
        Ok(rows.iter().map(|r| FunctionMeta {
            name: r.try_get(0).unwrap_or_default(),
            return_type: r.try_get(1).unwrap_or_default(),
            language: r.try_get(2).unwrap_or_default(),
        }).collect())
    }

    async fn get_relationships(&self, _database: &str, schema: &str) -> Result<Vec<Relationship>, AppError> {
        let rows = sqlx::query(
            "SELECT kcu.table_name, kcu.column_name, ccu.table_name, ccu.column_name, rc.constraint_name \
             FROM information_schema.referential_constraints rc \
             JOIN information_schema.key_column_usage kcu \
               ON kcu.constraint_name = rc.constraint_name AND kcu.constraint_schema = rc.constraint_schema \
             JOIN information_schema.constraint_column_usage ccu \
               ON ccu.constraint_name = rc.unique_constraint_name AND ccu.constraint_schema = rc.unique_constraint_schema \
             WHERE kcu.table_schema = $1"
        ).bind(schema).fetch_all(&self.pool).await.unwrap_or_default();
        Ok(rows.iter().map(|r| Relationship {
            from_table: r.try_get(0).unwrap_or_default(),
            from_column: r.try_get(1).unwrap_or_default(),
            to_table: r.try_get(2).unwrap_or_default(),
            to_column: r.try_get(3).unwrap_or_default(),
            constraint_name: r.try_get(4).unwrap_or_default(),
        }).collect())
    }

    async fn query_page(&self, sql: &str, offset: u64, limit: u32) -> Result<QueryResult, AppError> {
        let sql = sql.trim().trim_end_matches(';').trim();
        let wrapped = format!("SELECT * FROM ({sql}) AS _anide_q LIMIT {limit} OFFSET {offset}");
        let start = Instant::now();
        let rows = sqlx::query(&wrapped).fetch_all(&self.pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let columns: Vec<String> = rows.first()
            .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
            .unwrap_or_default();

        let result_rows = rows.iter()
            .map(|r| (0..r.len()).map(|i| pg_to_json(r, i)).collect())
            .collect();

        Ok(QueryResult { columns, rows: result_rows, duration_ms })
    }

    async fn count_table(&self, _database: &str, schema: &str, table: &str) -> Result<u64, AppError> {
        let schema = schema.replace('"', "\"\"");
        let table = table.replace('"', "\"\"");
        let sql = format!("SELECT COUNT(*) FROM \"{schema}\".\"{table}\"");
        let row = sqlx::query(&sql).fetch_one(&self.pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        Ok(row.try_get::<i64, _>(0).unwrap_or(0) as u64)
    }

    async fn execute_update(&self, op: &UpdateOp) -> Result<ExecutedOp, AppError> {
        let sql = preview_update_sql(op);
        let res = sqlx::query(&sql).execute(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(ExecutedOp { sql, affected: res.rows_affected() })
    }

    async fn execute_insert(&self, op: &InsertOp) -> Result<ExecutedOp, AppError> {
        let sql = preview_insert_sql(op);
        let res = sqlx::query(&sql).execute(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(ExecutedOp { sql, affected: res.rows_affected() })
    }

    async fn execute_delete(&self, op: &DeleteOp) -> Result<ExecutedOp, AppError> {
        let sql = preview_delete_sql(op);
        let res = sqlx::query(&sql).execute(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(ExecutedOp { sql, affected: res.rows_affected() })
    }
}

fn pg_to_json(row: &PgRow, i: usize) -> serde_json::Value {
    let type_name = row.column(i).type_info().name();
    match type_name {
        "BOOL" => row.try_get::<Option<bool>, _>(i).ok().flatten()
            .map(serde_json::Value::Bool).unwrap_or(serde_json::Value::Null),
        "INT2" => row.try_get::<Option<i16>, _>(i).ok().flatten()
            .map(|n| serde_json::json!(n)).unwrap_or(serde_json::Value::Null),
        "INT4" => row.try_get::<Option<i32>, _>(i).ok().flatten()
            .map(|n| serde_json::json!(n)).unwrap_or(serde_json::Value::Null),
        "INT8" | "OID" => row.try_get::<Option<i64>, _>(i).ok().flatten()
            .map(|n| serde_json::json!(n)).unwrap_or(serde_json::Value::Null),
        "FLOAT4" => row.try_get::<Option<f32>, _>(i).ok().flatten()
            .map(|f| serde_json::json!(f)).unwrap_or(serde_json::Value::Null),
        "FLOAT8" | "NUMERIC" => row.try_get::<Option<f64>, _>(i).ok().flatten()
            .map(|f| serde_json::json!(f)).unwrap_or(serde_json::Value::Null),
        "JSON" | "JSONB" => row.try_get::<Option<serde_json::Value>, _>(i).ok().flatten()
            .unwrap_or(serde_json::Value::Null),
        _ => row.try_get::<Option<String>, _>(i).ok().flatten()
            .map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
    }
}
