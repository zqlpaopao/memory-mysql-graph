use anyhow::Result;
use log::error;
use mysql_async::Result as MySqlResult;
use mysql_async::prelude::Query;
use mysql_async::{Opts, OptsBuilder, Pool, Row};
use mysql_binlog_connector_rust::binlog_client::BinlogClient;
use mysql_binlog_connector_rust::event::event_data::EventData;
use mysql_binlog_connector_rust::event::row_event::RowEvent;
use mysql_binlog_connector_rust::event::table_map_event::TableMapEvent;
use std::collections::HashMap;

/// Options for translating points or edges, library names,
/// table name conversion functions
/// Processed functions
pub struct Item<T> {
    pub db_name: String,
    pub table: String,
    pub tidy_func: Box<dyn Fn(i8, Vec<RowEvent>) -> Option<T> + Send + Sync>,
    pub func: Box<dyn Fn(i8, Option<T>) + Send + Sync>,
}

/// Used to monitor changes in the binlog of MySQL
pub struct WatchMysqlBinlog<T, R> {
    node: Item<T>,
    edge: Item<R>,
    url: String,
    heartbeat_interval_secs: u64,
    timeout_secs: u64,
    server_id: u64,
    table_map_cache: HashMap<u64, TableMapEvent>,
}

impl<T, R> WatchMysqlBinlog<T, R>
where
    T: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    /// Create an instance to monitor MySQL binlog
    pub fn new(
        node: Item<T>,
        edge: Item<R>,
        url: String,
        heartbeat_interval_secs: u64,
        timeout_secs: u64,
        server_id: u64,
    ) -> WatchMysqlBinlog<T, R> {
        WatchMysqlBinlog {
            node,
            edge,
            url,
            heartbeat_interval_secs,
            timeout_secs,
            server_id,
            table_map_cache: HashMap::with_capacity(2),
        }
    }

    /// Enable time monitoring
    pub async fn run(&mut self) -> Result<()> {
        let (gt_id, gt_id_set, filename, position) = self.create_binlog_stream_option().await?;
        let binlog_filename = String::from_utf8(filename)?;
        let mut client = BinlogClient {
            url: self.url.clone(),
            binlog_filename,
            binlog_position: position as u32,
            server_id: self.server_id,
            gtid_enabled: gt_id,
            gtid_set: gt_id_set,
            heartbeat_interval_secs: self.heartbeat_interval_secs,
            timeout_secs: self.timeout_secs,
        };
        let mut stream = client.connect().await?;
        loop {
            match stream.read().await {
                Ok((_, data)) => {
                    self.parse_json_columns(data).await;
                }
                Err(e) => {
                    error!("Error reading from stream: {}", e);
                }
            }
        }
    }

    /// Distribute changes in parsing time to specific processing logic
    async fn parse_json_columns(&mut self, data: EventData) {
        match data {
            EventData::WriteRows(event) => {
                self.tidy_func(event.table_id, 1, event.rows).await;
            }
            EventData::DeleteRows(event) => {
                self.tidy_func(event.table_id, 3, event.rows).await;
            }
            EventData::UpdateRows(event) => {
                for (before, after) in event.rows {
                    self.tidy_func(event.table_id, 2, vec![before, after]).await;
                }
            }
            EventData::TableMap(event) => {
                if event.database_name != self.node.db_name
                    && event.database_name != self.edge.db_name
                {
                    return;
                }
                if event.table_name != self.node.table && event.table_name != self.edge.table {
                    return;
                }

                self.table_map_cache.entry(event.table_id).or_insert(event);
            }
            _ => {}
        }
    }

    /// Processing function, only handles the incoming table and db
    async fn tidy_func(&self, table_id: u64, tag: i8, data: Vec<RowEvent>) {
        let table = self.table_map_cache.get(&table_id);
        if table.is_none() {
            return;
        }
        let table = table.unwrap();
        if table.database_name == self.node.db_name && table.table_name == self.node.table {
            let res = (self.node.tidy_func)(tag, data);
            (self.node.func)(tag, res);
        } else if table.database_name == self.edge.db_name && table.table_name == self.edge.table {
            let res = (self.edge.tidy_func)(tag, data);
            (self.edge.func)(tag, res);
        }
    }

    /// Create monitoring option link URL and file location for GT ID, GT ID SET offset, and binlog
    async fn create_binlog_stream_option(&self) -> MySqlResult<(bool, String, Vec<u8>, u64)> {
        let mut conn = Pool::new(OptsBuilder::from_opts(Opts::from_url(self.url.as_str())?))
            .get_conn()
            .await?;

        if conn.server_version() >= (8, 0, 31) && conn.server_version() < (9, 0, 0) {
            let _ = "SET binlog_transaction_compression=ON"
                .ignore(&mut conn)
                .await;
        }
        let mut gt_id = false;
        let mut gt_id_set = String::new();
        let mut filename = Vec::new();
        let mut position = 0;
        if let Ok(Some(gt_id_mode)) = "SELECT @@GLOBAL.GTID_MODE"
            .first::<String, _>(&mut conn)
            .await
        {
            if gt_id_mode.starts_with("ON") {
                gt_id = true;
                let row: Option<Row> = "SHOW MASTER STATUS".first(&mut conn).await?;
                if let Some(row) = row {
                    gt_id_set = row.get("Executed_Gtid_Set").unwrap_or_default();
                }
            }
        }
        let row: Vec<Row> = "SHOW BINARY LOGS".fetch(&mut conn).await?;
        if let Some(row) = row.last() {
            filename = row.get("Log_name").unwrap_or_default();
            position = row.get("File_size").unwrap_or(0);
        }
        Ok((gt_id, gt_id_set, filename, position))
    }
}
