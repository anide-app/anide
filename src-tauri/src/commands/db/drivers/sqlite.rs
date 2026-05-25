use super::{DbConnection, DbDriver};
use crate::commands::db::*;
use crate::error::AppError;
use async_trait::async_trait;
use sqlx::{Column, Row, TypeInfo};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use std::collections::HashMap;
use std::time::Instant;

pub struct SqliteDriver;

#[async_trait]
impl DbDriver for SqliteDriver {
    fn display_name(&self) -> &'static str { "SQLite" }
    fn config_type(&self) -> &'static str { "sqlite" }

    fn tree_structure(&self) -> TreeStructure {
        TreeStructure {
            levels: vec![
                TreeLevel { label: "Table".into(), icon: "table".into() },
            ],
        }
    }

    async fn connect(&self, config: &ConnectionConfig, env_vars: &HashMap<String, String>) -> Result<Box<dyn DbConnection>, AppError> {
        let pool = sqlite_pool(config, env_vars, 3).await?;
        Ok(Box::new(LiteConn { pool }))
    }

    async fn test_connection(&self, config: &ConnectionConfig, env_vars: &HashMap<String, String>) -> Result<TestResult, AppError> {
        let start = Instant::now();
        let pool = sqlite_pool(config, env_vars, 1).await?;
        let latency_ms = start.elapsed().as_millis() as u64;
        let row = sqlx::query("SELECT sqlite_version()").fetch_one(&pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        pool.close().await;
        Ok(TestResult { latency_ms, server_version: format!("SQLite {version}") })
    }
}

async fn sqlite_pool(config: &ConnectionConfig, env_vars: &HashMap<String, String>, max_conns: u32) -> Result<SqlitePool, AppError> {
    let raw = config.path.as_deref()
        .or(config.connection_string.as_deref())
        .unwrap_or(":memory:");
    let resolved = resolve_conn_template(raw, env_vars);
    let resolved = resolved.trim();

    let opts = if resolved == ":memory:" {
        SqliteConnectOptions::new().in_memory(true)
    } else {
        let p = std::path::Path::new(resolved);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else if !config.project_path.is_empty() {
            std::path::Path::new(&config.project_path).join(p)
        } else {
            p.to_path_buf()
        };
        SqliteConnectOptions::new().filename(&abs).create_if_missing(false)
    };

    SqlitePoolOptions::new()
        .max_connections(max_conns)
        .connect_with(opts)
        .await
        .map_err(|e| AppError::Db(format!("SQLite: {e}")))
}

pub struct LiteConn { pool: SqlitePool }

#[async_trait]
impl DbConnection for LiteConn {
    fn driver_type(&self) -> &'static str { "sqlite" }

    async fn list_databases(&self) -> Result<Vec<String>, AppError> {
        Ok(vec!["main".to_string()])
    }

    async fn list_schemas(&self, _database: &str) -> Result<Vec<String>, AppError> {
        Ok(vec!["main".to_string()])
    }

    async fn list_tables(&self, _database: &str, _schema: &str) -> Result<Vec<TableMeta>, AppError> {
        let rows = sqlx::query(
            "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name"
        ).fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| TableMeta {
            name: r.try_get(0).unwrap_or_default(),
            table_type: r.try_get::<String, _>(1).map(|t| t.to_uppercase()).unwrap_or_default(),
        }).collect())
    }

    async fn list_columns(&self, _database: &str, _schema: &str, table: &str) -> Result<Vec<ColumnMeta>, AppError> {
        let sql = format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\""));
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| {
            let pk: i64 = r.try_get(5).unwrap_or(0);
            let notnull: i64 = r.try_get(3).unwrap_or(0);
            ColumnMeta {
                name: r.try_get(1).unwrap_or_default(),
                col_type: r.try_get(2).unwrap_or_default(),
                nullable: notnull == 0,
                default_val: r.try_get::<Option<String>, _>(4).unwrap_or(None),
                is_primary: pk > 0,
                is_unique: false,
            }
        }).collect())
    }

    async fn list_indexes(&self, _database: &str, _schema: &str, table: &str) -> Result<Vec<IndexMeta>, AppError> {
        let sql = format!("PRAGMA index_list(\"{}\")", table.replace('"', "\"\""));
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.unwrap_or_default();
        let mut result = Vec::new();
        for r in &rows {
            let name: String = r.try_get(1).unwrap_or_default();
            let unique: i64 = r.try_get(2).unwrap_or(0);
            let info_sql = format!("PRAGMA index_info(\"{}\")", name.replace('"', "\"\""));
            let info_rows = sqlx::query(&info_sql).fetch_all(&self.pool).await.unwrap_or_default();
            let columns: Vec<String> = info_rows.iter()
                .map(|ir| ir.try_get::<String, _>(2).unwrap_or_default())
                .collect();
            result.push(IndexMeta { name, is_unique: unique != 0, is_primary: false, columns });
        }
        Ok(result)
    }

    async fn list_views(&self, _database: &str, _schema: &str) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query("SELECT name FROM sqlite_master WHERE type = 'view' ORDER BY name")
            .fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect())
    }

    async fn list_functions(&self, _database: &str, _schema: &str) -> Result<Vec<FunctionMeta>, AppError> {
        Ok(Vec::new())
    }

    async fn get_relationships(&self, _database: &str, _schema: &str) -> Result<Vec<Relationship>, AppError> {
        Ok(Vec::new())
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
            .map(|r| (0..r.len()).map(|i| lite_to_json(r, i)).collect())
            .collect();

        Ok(QueryResult { columns, rows: result_rows, duration_ms })
    }

    async fn count_table(&self, _database: &str, _schema: &str, table: &str) -> Result<u64, AppError> {
        let sql = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
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

fn lite_to_json(row: &SqliteRow, i: usize) -> serde_json::Value {
    let type_name = row.column(i).type_info().name();
    match type_name {
        "BOOLEAN" | "BOOL" => row.try_get::<Option<bool>, _>(i).ok().flatten()
            .map(serde_json::Value::Bool).unwrap_or(serde_json::Value::Null),
        "INTEGER" | "INT" | "TINYINT" | "SMALLINT" | "BIGINT" =>
            row.try_get::<Option<i64>, _>(i).ok().flatten()
            .map(|n| serde_json::json!(n)).unwrap_or(serde_json::Value::Null),
        "REAL" | "FLOAT" | "DOUBLE" | "NUMERIC" =>
            row.try_get::<Option<f64>, _>(i).ok().flatten()
            .map(|f| serde_json::json!(f)).unwrap_or(serde_json::Value::Null),
        _ => row.try_get::<Option<String>, _>(i).ok().flatten()
            .map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
    }
}
