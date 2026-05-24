use super::{DbConnection, DbDriver};
use crate::commands::db::*;
use crate::error::AppError;
use async_trait::async_trait;
use sqlx::{Column, Row, TypeInfo};
use sqlx::mysql::{MySqlPool, MySqlPoolOptions, MySqlRow};
use std::collections::HashMap;
use std::time::Instant;

pub struct MysqlDriver;

#[async_trait]
impl DbDriver for MysqlDriver {
    fn display_name(&self) -> &'static str { "MySQL / MariaDB" }
    fn config_type(&self) -> &'static str { "mysql" }

    fn tree_structure(&self) -> TreeStructure {
        TreeStructure {
            levels: vec![
                TreeLevel { label: "Database".into(), icon: "database".into() },
                TreeLevel { label: "Table".into(), icon: "table".into() },
            ],
        }
    }

    async fn connect(&self, config: &ConnectionConfig, env_vars: &HashMap<String, String>) -> Result<Box<dyn DbConnection>, AppError> {
        let url = my_url(config, env_vars);
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .map_err(|e| AppError::Db(format!("MySQL: {e}")))?;
        Ok(Box::new(MyConn { pool }))
    }

    async fn test_connection(&self, config: &ConnectionConfig, env_vars: &HashMap<String, String>) -> Result<TestResult, AppError> {
        let url = my_url(config, env_vars);
        let start = Instant::now();
        let pool = MySqlPoolOptions::new().max_connections(1).connect(&url).await
            .map_err(|e| AppError::Db(format!("MySQL: {e}")))?;
        let latency_ms = start.elapsed().as_millis() as u64;
        let row = sqlx::query("SELECT VERSION()").fetch_one(&pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        pool.close().await;
        Ok(TestResult { latency_ms, server_version: version })
    }
}

fn my_url(config: &ConnectionConfig, env: &HashMap<String, String>) -> String {
    if let Some(cs) = &config.connection_string {
        return resolve_conn_template(cs, env);
    }
    let host = resolve_conn_template(config.host.as_deref().unwrap_or("localhost"), env);
    let port = resolve_conn_template(config.port.as_deref().unwrap_or("3306"), env);
    let db   = resolve_conn_template(config.database.as_deref().unwrap_or(""), env);
    let user = resolve_conn_template(config.username.as_deref().unwrap_or("root"), env);
    let pass = resolve_conn_template(config.password.as_deref().unwrap_or(""), env);
    format!("mysql://{}:{}@{}:{}/{}", urlencoding::encode(&user), urlencoding::encode(&pass), host, port, db)
}

pub struct MyConn { pool: MySqlPool }

#[async_trait]
impl DbConnection for MyConn {
    fn driver_type(&self) -> &'static str { "mysql" }

    async fn list_databases(&self) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query("SHOW DATABASES").fetch_all(&self.pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect())
    }

    async fn list_schemas(&self, database: &str) -> Result<Vec<String>, AppError> {
        // MySQL has no schemas; return database as a single-element "schema"
        Ok(vec![database.to_string()])
    }

    async fn list_tables(&self, database: &str, _schema: &str) -> Result<Vec<TableMeta>, AppError> {
        let sql = format!("SELECT table_name, table_type FROM information_schema.tables WHERE table_schema = '{}' ORDER BY table_name", database.replace('\'', "''"));
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| TableMeta {
            name: r.try_get(0).unwrap_or_default(),
            table_type: r.try_get(1).unwrap_or_default(),
        }).collect())
    }

    async fn list_columns(&self, database: &str, _schema: &str, table: &str) -> Result<Vec<ColumnMeta>, AppError> {
        let sql = format!(
            "SELECT column_name, column_type, is_nullable, column_default, column_key \
             FROM information_schema.columns WHERE table_schema = '{}' AND table_name = '{}' \
             ORDER BY ordinal_position",
            database.replace('\'', "''"), table.replace('\'', "''")
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await
            .map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| {
            let key: String = r.try_get(4).unwrap_or_default();
            ColumnMeta {
                name: r.try_get(0).unwrap_or_default(),
                col_type: r.try_get(1).unwrap_or_default(),
                nullable: r.try_get::<String, _>(2).unwrap_or_default() == "YES",
                default_val: r.try_get::<Option<String>, _>(3).unwrap_or(None),
                is_primary: key == "PRI",
                is_unique: key == "UNI",
            }
        }).collect())
    }

    async fn list_indexes(&self, database: &str, _schema: &str, table: &str) -> Result<Vec<IndexMeta>, AppError> {
        let sql = format!(
            "SELECT index_name, non_unique, column_name FROM information_schema.statistics \
             WHERE table_schema = '{}' AND table_name = '{}' ORDER BY index_name, seq_in_index",
            database.replace('\'', "''"), table.replace('\'', "''")
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.unwrap_or_default();
        let mut map: std::collections::HashMap<String, IndexMeta> = std::collections::HashMap::new();
        for r in &rows {
            let name: String = r.try_get(0).unwrap_or_default();
            let non_unique: i64 = r.try_get(1).unwrap_or(1);
            let entry = map.entry(name.clone()).or_insert(IndexMeta {
                is_primary: name == "PRIMARY",
                is_unique: non_unique == 0,
                name,
                columns: Vec::new(),
            });
            let col: String = r.try_get(2).unwrap_or_default();
            if !col.is_empty() { entry.columns.push(col); }
        }
        Ok(map.into_values().collect())
    }

    async fn list_views(&self, database: &str, _schema: &str) -> Result<Vec<String>, AppError> {
        let sql = format!("SELECT table_name FROM information_schema.views WHERE table_schema = '{}' ORDER BY table_name", database.replace('\'', "''"));
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
        Ok(rows.iter().map(|r| r.try_get::<String, _>(0).unwrap_or_default()).collect())
    }

    async fn list_functions(&self, database: &str, _schema: &str) -> Result<Vec<FunctionMeta>, AppError> {
        let sql = format!(
            "SELECT routine_name, dtd_identifier, routine_body FROM information_schema.routines \
             WHERE routine_schema = '{}' AND routine_type = 'FUNCTION' ORDER BY routine_name",
            database.replace('\'', "''")
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.unwrap_or_default();
        Ok(rows.iter().map(|r| FunctionMeta {
            name: r.try_get(0).unwrap_or_default(),
            return_type: r.try_get(1).unwrap_or_default(),
            language: r.try_get(2).unwrap_or_default(),
        }).collect())
    }

    async fn get_relationships(&self, database: &str, _schema: &str) -> Result<Vec<Relationship>, AppError> {
        let sql = format!(
            "SELECT kcu.table_name, kcu.column_name, kcu.referenced_table_name, kcu.referenced_column_name, kcu.constraint_name \
             FROM information_schema.key_column_usage kcu \
             WHERE kcu.table_schema = '{}' AND kcu.referenced_table_name IS NOT NULL",
            database.replace('\'', "''")
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.unwrap_or_default();
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
            .map(|r| (0..r.len()).map(|i| my_to_json(r, i)).collect())
            .collect();

        Ok(QueryResult { columns, rows: result_rows, duration_ms })
    }

    async fn count_table(&self, database: &str, _schema: &str, table: &str) -> Result<u64, AppError> {
        let sql = format!("SELECT COUNT(*) FROM `{}`.`{}`", database.replace('`', ""), table.replace('`', ""));
        let row = sqlx::query(&sql).fetch_one(&self.pool).await.map_err(|e| AppError::Db(e.to_string()))?;
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

fn my_to_json(row: &MySqlRow, i: usize) -> serde_json::Value {
    let type_name = row.column(i).type_info().name();
    match type_name {
        "BOOLEAN" | "TINYINT(1)" => row.try_get::<Option<bool>, _>(i).ok().flatten()
            .map(serde_json::Value::Bool).unwrap_or(serde_json::Value::Null),
        "TINYINT" | "SMALLINT" | "INT" | "BIGINT" | "MEDIUMINT" | "YEAR" =>
            row.try_get::<Option<i64>, _>(i).ok().flatten()
            .map(|n| serde_json::json!(n)).unwrap_or(serde_json::Value::Null),
        "FLOAT" | "DOUBLE" | "DECIMAL" | "NUMERIC" =>
            row.try_get::<Option<f64>, _>(i).ok().flatten()
            .map(|f| serde_json::json!(f)).unwrap_or(serde_json::Value::Null),
        "JSON" => row.try_get::<Option<serde_json::Value>, _>(i).ok().flatten()
            .unwrap_or(serde_json::Value::Null),
        _ => row.try_get::<Option<String>, _>(i).ok().flatten()
            .map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
    }
}
