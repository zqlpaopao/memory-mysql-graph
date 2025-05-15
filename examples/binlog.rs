use memory_mysql_graph::controller::mysql_binlog::{Item, WatchMysqlBinlog};
use mysql_binlog_connector_rust::column::column_value::ColumnValue;
use mysql_binlog_connector_rust::column::json::json_binary::JsonBinary;
use mysql_binlog_connector_rust::event::row_event::RowEvent;

#[tokio::main]
async fn main() {
    let mut watch_mysql_binlog: WatchMysqlBinlog<User, User> = WatchMysqlBinlog::new(
        Item {
            db_name: "test".to_string(),
            table: "users".to_string(),
            tidy_func: Box::new(tidy_func),
            func: Box::new(user),
        },
        Item {
            db_name: "test".to_string(),
            table: "".to_string(),
            tidy_func: Box::new(tidy_func),
            func: Box::new(user),
        },
        "mysql://root:meimima123@127.0.0.1:3306/test".to_string(),
        60,
        30,
        200,
    );

    watch_mysql_binlog.run().await.unwrap();
}

#[derive(Debug)]
pub struct User {
    id: u64,
    name: String,
    hair_color: String,
    created_at: i64,
    update_at: i64,
}

fn tidy_func(tag: i8, data: Vec<RowEvent>) -> Option<User> {
    let mut user = User {
        id: 0,
        name: "".to_string(),
        hair_color: "".to_string(),
        created_at: 0,
        update_at: 0,
    };
    if (tag == 1 || tag == 3) && data.len() < 1 {
        return None;
    } else if tag == 2 && data.len() < 2 {
        return None;
    }

    let res;
    if tag == 1 || tag == 3 {
        res = &data[0].column_values;
    } else if tag == 2 {
        res = &data[1].column_values;
    } else {
        return None;
    }

    for (key, column_value) in res.iter().enumerate() {
        if key == 0 {
            if let ColumnValue::Long(bytes) = column_value {
                user.id = (*bytes) as u64
            }
        }

        if key == 1 {
            if let ColumnValue::Blob(bytes) = column_value {
                user.name = JsonBinary::parse_as_string(&bytes).unwrap_or_default();
            }
        }
        if key == 2 {
            if let ColumnValue::Blob(bytes) = column_value {
                user.hair_color = JsonBinary::parse_as_string(&bytes).unwrap_or_default();
            }
        }

        if key == 3 {
            if let ColumnValue::Timestamp(bytes) = column_value {
                user.created_at = *bytes;
            }
        }

        if key == 4 {
            if let ColumnValue::Timestamp(bytes) = column_value {
                user.update_at = *bytes;
            }
        }
    }

    Some(user)
}

fn user(tag: i8, user: Option<User>) {
    if user.is_none() {
        return;
    }
    println!(" {}  {:?}", tag, user);
}
