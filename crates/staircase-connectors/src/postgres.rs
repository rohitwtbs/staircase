//! PostgreSQL output connector (blueprint).

use async_trait::async_trait;
use serde::Deserialize;
use staircase_core::error::Result;
use staircase_core::model::DataPoint;
use staircase_core::traits::DataPublisher;

use crate::not_implemented;

/// Configuration for the PostgreSQL connector.
#[derive(Debug, Clone, Deserialize)]
pub struct PostgresConnectorConfig {
    /// Connection string (e.g. `postgres://user:pass@host/db`).
    pub dsn: String,
    /// Target table for inserts.
    #[serde(default = "default_table")]
    pub table: String,
}

fn default_table() -> String {
    "data_points".to_string()
}

/// Inserts data points into a PostgreSQL table.
pub struct PostgresConnector {
    config: PostgresConnectorConfig,
    // TODO: async SQL client/pool (e.g. tokio-postgres Client or sqlx Pool).
}

impl PostgresConnector {
    /// Build a connector from its configuration.
    pub fn new(config: PostgresConnectorConfig) -> Self {
        Self { config }
    }

    /// The connector configuration.
    pub fn config(&self) -> &PostgresConnectorConfig {
        &self.config
    }
}

#[async_trait]
impl DataPublisher for PostgresConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: connect/pool against `config.dsn`; ensure the table exists.
        Err(not_implemented("postgres::connect"))
    }

    async fn publish(&mut self, points: &[DataPoint]) -> Result<()> {
        // TODO: map each point to a row (source, protocol, device_id, tag_name,
        // value, quality, ts) and batch-insert into `config.table` in one
        // transaction so the whole batch commits or rolls back together.
        let _ = points;
        Err(not_implemented("postgres::publish"))
    }
}
