//! TimescaleDB output connector (blueprint).
//!
//! TimescaleDB is PostgreSQL-compatible, so this connector targets a Postgres
//! DSN but inserts into a hypertable (time-partitioned) and is tuned for
//! high-throughput, time-ordered writes.

use async_trait::async_trait;
use serde::Deserialize;
use staircase_core::error::Result;
use staircase_core::model::DataPoint;
use staircase_core::traits::DataPublisher;

use crate::not_implemented;

/// Configuration for the TimescaleDB connector.
#[derive(Debug, Clone, Deserialize)]
pub struct TimescaleConnectorConfig {
    /// Connection string (Postgres-compatible DSN).
    pub dsn: String,
    /// Target hypertable for inserts.
    #[serde(default = "default_hypertable")]
    pub hypertable: String,
}

fn default_hypertable() -> String {
    "data_points".to_string()
}

/// Inserts data points into a TimescaleDB hypertable.
pub struct TimescaleConnector {
    config: TimescaleConnectorConfig,
    // TODO: async SQL client/pool (same family as the Postgres connector).
}

impl TimescaleConnector {
    /// Build a connector from its configuration.
    pub fn new(config: TimescaleConnectorConfig) -> Self {
        Self { config }
    }

    /// The connector configuration.
    pub fn config(&self) -> &TimescaleConnectorConfig {
        &self.config
    }
}

#[async_trait]
impl DataPublisher for TimescaleConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: connect against `config.dsn`; ensure the hypertable exists
        // (create_hypertable) with `timestamp` as the time dimension.
        Err(not_implemented("timescale::connect"))
    }

    async fn publish(&mut self, points: &[DataPoint]) -> Result<()> {
        // TODO: batch-insert rows into `config.hypertable`, ideally via a
        // multi-row INSERT or COPY for hypertable-friendly throughput.
        let _ = points;
        Err(not_implemented("timescale::publish"))
    }
}
