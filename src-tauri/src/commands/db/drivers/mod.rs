use crate::commands::db::*;
use crate::error::AppError;
use async_trait::async_trait;
use std::collections::HashMap;

// ── Traits ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait DbDriver: Send + Sync {
    fn display_name(&self) -> &'static str;
    fn config_type(&self) -> &'static str;
    fn tree_structure(&self) -> TreeStructure;
    async fn connect(
        &self,
        config: &ConnectionConfig,
        env_vars: &HashMap<String, String>,
    ) -> Result<Box<dyn DbConnection>, AppError>;
    async fn test_connection(
        &self,
        config: &ConnectionConfig,
        env_vars: &HashMap<String, String>,
    ) -> Result<TestResult, AppError>;
}

#[async_trait]
pub trait DbConnection: Send + Sync {
    fn driver_type(&self) -> &'static str;
    async fn list_databases(&self) -> Result<Vec<String>, AppError>;
    async fn list_schemas(&self, database: &str) -> Result<Vec<String>, AppError>;
    async fn list_tables(&self, database: &str, schema: &str) -> Result<Vec<TableMeta>, AppError>;
    async fn list_columns(&self, database: &str, schema: &str, table: &str) -> Result<Vec<ColumnMeta>, AppError>;
    async fn list_indexes(&self, database: &str, schema: &str, table: &str) -> Result<Vec<IndexMeta>, AppError>;
    async fn list_views(&self, database: &str, schema: &str) -> Result<Vec<String>, AppError>;
    async fn list_functions(&self, database: &str, schema: &str) -> Result<Vec<FunctionMeta>, AppError>;
    async fn get_relationships(&self, database: &str, schema: &str) -> Result<Vec<Relationship>, AppError>;
    async fn query_page(&self, sql: &str, offset: u64, limit: u32) -> Result<QueryResult, AppError>;
    async fn count_table(&self, database: &str, schema: &str, table: &str) -> Result<u64, AppError>;
    async fn execute_update(&self, op: &UpdateOp) -> Result<ExecutedOp, AppError>;
    async fn execute_insert(&self, op: &InsertOp) -> Result<ExecutedOp, AppError>;
    async fn execute_delete(&self, op: &DeleteOp) -> Result<ExecutedOp, AppError>;
}

// ── Driver modules ────────────────────────────────────────────────────────────

mod postgres;
mod mysql;
mod sqlite;
mod mongo;

pub use postgres::PostgresDriver;
pub use mysql::MysqlDriver;
pub use sqlite::SqliteDriver;
pub use mongo::MongoDriver;

// ── Registry ──────────────────────────────────────────────────────────────────

pub fn get_driver_for(conn_type: &str) -> Option<Box<dyn DbDriver>> {
    match conn_type {
        "postgresql" | "postgres" => Some(Box::new(PostgresDriver)),
        "mysql" | "mariadb" => Some(Box::new(MysqlDriver)),
        "sqlite" => Some(Box::new(SqliteDriver)),
        "mongodb" => Some(Box::new(MongoDriver)),
        _ => None,
    }
}
