use super::{DbConnection, DbDriver};
use crate::commands::db::*;
use crate::error::AppError;
use async_trait::async_trait;
use std::collections::HashMap;

pub struct MongoDriver;

#[async_trait]
impl DbDriver for MongoDriver {
    fn display_name(&self) -> &'static str { "MongoDB" }
    fn config_type(&self) -> &'static str { "mongodb" }

    fn tree_structure(&self) -> TreeStructure {
        TreeStructure {
            levels: vec![
                TreeLevel { label: "Database".into(), icon: "database".into() },
                TreeLevel { label: "Collection".into(), icon: "table".into() },
            ],
        }
    }

    async fn connect(&self, _config: &ConnectionConfig, _env_vars: &HashMap<String, String>) -> Result<Box<dyn DbConnection>, AppError> {
        Err(AppError::Db("MongoDB driver not yet implemented. Coming soon.".into()))
    }

    async fn test_connection(&self, _config: &ConnectionConfig, _env_vars: &HashMap<String, String>) -> Result<TestResult, AppError> {
        Err(AppError::Db("MongoDB driver not yet implemented. Coming soon.".into()))
    }
}

// Stub connection — only needed to satisfy the type system; never actually constructed
pub struct MongoConn;

#[async_trait]
impl DbConnection for MongoConn {
    fn driver_type(&self) -> &'static str { "mongodb" }
    async fn list_databases(&self) -> Result<Vec<String>, AppError> { not_impl() }
    async fn list_schemas(&self, _: &str) -> Result<Vec<String>, AppError> { not_impl() }
    async fn list_tables(&self, _: &str, _: &str) -> Result<Vec<TableMeta>, AppError> { not_impl() }
    async fn list_columns(&self, _: &str, _: &str, _: &str) -> Result<Vec<ColumnMeta>, AppError> { not_impl() }
    async fn list_indexes(&self, _: &str, _: &str, _: &str) -> Result<Vec<IndexMeta>, AppError> { not_impl() }
    async fn list_views(&self, _: &str, _: &str) -> Result<Vec<String>, AppError> { not_impl() }
    async fn list_functions(&self, _: &str, _: &str) -> Result<Vec<FunctionMeta>, AppError> { not_impl() }
    async fn get_relationships(&self, _: &str, _: &str) -> Result<Vec<Relationship>, AppError> { not_impl() }
    async fn query_page(&self, _: &str, _: u64, _: u32) -> Result<QueryResult, AppError> { not_impl() }
    async fn count_table(&self, _: &str, _: &str, _: &str) -> Result<u64, AppError> { not_impl() }
    async fn execute_update(&self, _: &UpdateOp) -> Result<ExecutedOp, AppError> { not_impl() }
    async fn execute_insert(&self, _: &InsertOp) -> Result<ExecutedOp, AppError> { not_impl() }
    async fn execute_delete(&self, _: &DeleteOp) -> Result<ExecutedOp, AppError> { not_impl() }
}

fn not_impl<T>() -> Result<T, AppError> {
    Err(AppError::Db("MongoDB not yet implemented".into()))
}
