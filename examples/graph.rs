use anyhow::Result;
use axum::Router;
use axum::routing::get;
use memory_mysql_graph::controller::graph::GraphInstance;
use memory_mysql_graph::controller::graph::NodeM as NM;
use memory_mysql_graph::controller::graph::{EdgeM as EM, EdgeM, NodeM};
use memory_mysql_graph::controller::mysql_binlog::{Item, WatchMysqlBinlog};
use mysql_binlog_connector_rust::column::column_value::ColumnValue;
use mysql_binlog_connector_rust::column::json::json_binary::JsonBinary;
use mysql_binlog_connector_rust::event::row_event::RowEvent;
use petgraph::algo::astar;
use petgraph::graph::NodeIndex;
use petgraph::prelude::EdgeRef;
use petgraph::{Graph, Undirected};
use rbatis::rbdc::pool::Pool;
use rbatis::{RBatis, crud};
use rbdc_mysql::MysqlDriver;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // write_data().await;

    let t = std::time::Instant::now();
    read_data().await;
    println!("加载数据代码执行时间: {:.2?}", t.elapsed()); // 输出: 代码执行时间: 2.00s

    tokio::spawn(async { binlog().await });

    // find().await;

    test_axum().await.unwrap();
}

async fn binlog() {
    let mut watch_mysql_binlog: WatchMysqlBinlog<NodeY, EdgeY> = WatchMysqlBinlog::new(
        Item {
            db_name: "test".to_string(),
            table: "node".to_string(),
            tidy_func: Box::new(tidy_func),
            func: Box::new(node),
        },
        Item {
            db_name: "test".to_string(),
            table: "edge".to_string(),
            tidy_func: Box::new(tidy_func_edge),
            func: Box::new(edge),
        },
        "mysql://root:meimima123@127.0.0.1:3306/test".to_string(),
        60,
        30,
        200,
    );

    watch_mysql_binlog.run().await.unwrap();
}

fn tidy_func(tag: i8, data: Vec<RowEvent>) -> Option<NodeY> {
    let mut user = NodeY {
        id: 0,

        vid: "".to_string(),
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
                user.id = *bytes
            }
        }

        if key == 1 {
            if let ColumnValue::String(bytes) = column_value {
                user.vid = JsonBinary::parse_as_string(&bytes).unwrap_or_default();
            }
        }
    }

    Some(user)
}

fn node(tag: i8, user: Option<NodeY>) {
    if user.is_none() {
        return;
    }
    tokio::spawn(async move {
        let user = user.unwrap();
        if tag == 1 {
            GRAPH
                .add_node(NodeM {
                    vid: user.vid.to_string(),
                    data: Default::default(),
                })
                .await
                .unwrap();
        } else if tag == 2 {
            GRAPH.update_node(&*user.vid, HashMap::new()).await;
        } else if tag == 3 {
            GRAPH.remove_node(&*user.vid).await;
        }

        println!(" {}  {:?}", tag, user);
    });
    // println!("node ---- {:?}",GRAPH.inner);
}

fn tidy_func_edge(tag: i8, data: Vec<RowEvent>) -> Option<EdgeY> {
    let mut user = EdgeY {
        id: 0,

        src_vid: "".to_string(),
        dst_vid: "".to_string(),
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

    println!("{:?}", res);

    for (key, column_value) in res.iter().enumerate() {
        if key == 0 {
            if let ColumnValue::Long(bytes) = column_value {
                user.id = *bytes
            }
        }

        if key == 1 {
            if let ColumnValue::String(bytes) = column_value {
                user.src_vid = JsonBinary::parse_as_string(&bytes).unwrap_or_default();
            }
        }
        if key == 2 {
            if let ColumnValue::String(bytes) = column_value {
                user.dst_vid = JsonBinary::parse_as_string(&bytes).unwrap_or_default();
            }
        }
    }

    Some(user)
}

fn edge(tag: i8, user: Option<EdgeY>) {
    if user.is_none() {
        return;
    }
    tokio::spawn(async move {
        let user = user.unwrap();
        if tag == 1 {
            println!(" {}  {:?}", tag, user);

            GRAPH
                .add_edge(
                    user.src_vid.as_str(),
                    user.dst_vid.as_str(),
                    EdgeM {
                        weight: 0,
                        src_vid: user.src_vid.to_string(),
                        dst_vid: "".to_string(),
                        data: Default::default(),
                    },
                )
                .await
                .unwrap();
        } else if tag == 2 {
            GRAPH
                .update_edge_by_node(user.src_vid.as_str(), user.dst_vid.as_str(), HashMap::new())
                .await
                .unwrap();
        } else if tag == 3 {
            let res = GRAPH
                .remove_edge_by_node(user.src_vid.as_str(), user.dst_vid.as_str())
                .await;
            println!(" {}  {:?} {}", tag, user, res);
        }

        // println!("edge ---- {:?}",GRAPH.inner);

        println!(" {}  {:?}", tag, user);
    });
}

/***************************************** axum *********************************/
pub async fn test_axum() -> Result<()> {
    let app = routes();
    // .layer(middleware::from_fn(middleware_log::logging_middleware));

    println!("axum Listening on 0.0.0.0:3000");

    let listener = TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await?;
    Ok(())
}

pub fn routes() -> Router {
    Router::new().route("/find", get(find))
}

async fn index() -> &'static str {
    "Hello world!"
}

/***************************************** 图查找 *********************************/

pub static GRAPH: LazyLock<GraphInstance<Undirected>> = LazyLock::new(|| {
    GraphInstance::new(
        100000,
        100000,
        Graph::<NM, EM, Undirected>::with_capacity(100000, 100000),
    )
});

// pub static GRAPH: LazyLock<GraphInstance<Directed>> = LazyLock::new(|| {
//     GraphInstance::new(
//         100000,
//         100000,
//         Graph::<NM, EM, Directed>::with_capacity(100000, 100000),
//     )
// });

async fn find() {
    let t = std::time::Instant::now();
    let graph = GRAPH.inner.read().await;
    let start_node = NodeIndex::new(0);
    let target_node = NodeIndex::new(99999);

    // 使用 astar 算法，启发式函数设为 |e| 0 表示退化为 Dijkstra 算法
    let result = astar(
        &*graph,
        start_node,
        |node| node == target_node, // 目标节点判断
        // |e| e.weight().weight,             // 边权重
        |e| 0, // 边权重
        |_| 0, // 启发式函数（设为 0 等价于 Dijkstra）
               // |node| manhattan_distance(&graph[node], &graph[target_node]).try_into().unwrap(), //曼哈顿算法
    );
    let read = t.elapsed();

    let t1 = std::time::Instant::now();
    // 处理结果
    if let Some((distance, path)) = result {
        println!("从 Router 到 Server 的最短距离: {}", distance);
        println!("路径:");

        // 输出路径中的节点
        for (i, node) in path.iter().enumerate() {
            println!("  {}: {:?}", i, graph[*node].vid);
        }

        // 输出路径中的边
        println!("经过的边:");
        for i in 0..path.len() - 1 {
            let src = path[i];
            let dst = path[i + 1];

            // 查找边权重
            for edge in graph.edges(src) {
                if edge.target() == dst {
                    println!(
                        "  {} -> {} (权重: {:?})",
                        graph[src].vid,
                        graph[dst].vid,
                        edge.weight().weight
                    );
                    break;
                }
            }
        }
    } else {
        println!("无法从 Router 到达 Server");
    }

    println!("查找代码执行时间: {:.2?}", read); // 输出: 代码执行时间: 2.00s

    println!("打印代码执行时间: {:.2?}", t1.elapsed()); // 输出: 代码执行时间: 2.00s
}

/***************************************** 初始化数据 *********************************/

async fn read_data() {
    let rb = init_db().await;
    // 处理节点 (如果 add_node 返回 Future)
    let nodes = NodeY::select_all(&rb).await;
    if let Ok(node_batch) = &nodes {
        for v in node_batch {
            // 如果 add_node 是异步操作，需要 await
            GRAPH
                .add_node(NM {
                    vid: v.vid.to_string(),
                    data: Default::default(),
                })
                .await
                .unwrap(); // 假设 add_node 返回 Future
        }
    }

    // 处理边 (顺序执行)
    let edges = EdgeY::select_all(&rb).await;
    if let Ok(edge_batch) = &edges {
        for v in edge_batch {
            GRAPH
                .add_edge(
                    v.src_vid.as_str(),
                    v.dst_vid.as_str(),
                    EM {
                        weight: v.id as usize,
                        src_vid: v.src_vid.to_string(),
                        dst_vid: v.dst_vid.to_string(),
                        data: Default::default(),
                    },
                )
                .await
                .unwrap();
        }
    }
}

/***************************************** 写数据 *********************************/

async fn write_data() {
    let rb = init_db().await;

    // let mut node = Vec::with_capacity(100000);
    // let mut edge = Vec::with_capacity(100000);

    for i in 0..100000 {
        NodeY::insert(
            &rb,
            &NodeY {
                id: 0,
                vid: i.to_string(),
            },
        )
        .await
        .unwrap();
        if i > 0 {
            // edge.push(Edge{
            //     id: 0,
            //     src_vid: (i-1).to_string(),
            //     dst_vid: i.to_string(),
            // });
            EdgeY::insert(
                &rb,
                &EdgeY {
                    id: 0,
                    src_vid: (i - 1).to_string(),
                    dst_vid: i.to_string(),
                },
            )
            .await
            .unwrap();
        }
    }
    // let l = node.len();
    //  Node::insert_batch(&rb, &*node, l as u64).await.unwrap();
    // Edge::insert_batch(&rb, &*edge, l as u64).await.unwrap();
}

/***************************************** 初始化db *********************************/
const DB_URL: &str = "mysql://root:meimima123@127.0.0.1:3306/test";
async fn init_db() -> RBatis {
    let rb = RBatis::new();
    rb.init(MysqlDriver {}, DB_URL).expect("mysql init fail");
    let pool = rb.get_pool().unwrap();
    set(pool).await;
    rb
}

async fn set(pool: &dyn Pool) {
    pool.set_conn_max_lifetime(Some(std::time::Duration::from_secs(86400)))
        .await;
    pool.set_timeout(Some(std::time::Duration::from_secs(6400)))
        .await;
    pool.set_max_idle_conns(3).await;
    pool.set_max_open_conns(10).await;
}

// CREATE TABLE `node` (
//   `id` int NOT NULL AUTO_INCREMENT,
//   `vid` varchar(255) NOT NULL DEFAULT '' COMMENT 'vid',
//   `created_at` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP,
//   `updated_at` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
//   PRIMARY KEY (`id`)
// ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb3;

// CREATE TABLE `edge`
// (
//     `id` int NOT NULL AUTO_INCREMENT,
//     `src_vid`          varchar(255) NOT NULL DEFAULT '' COMMENT 'vid',
//     `dst_vid`          varchar(255) NOT NULL DEFAULT '' COMMENT 'vid',
//     `created_at` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP,
//     `updated_at` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
//      PRIMARY KEY (`id`)
// ) ENGINE=InnoDB DEFAULT CHARSET=utf8  ;

crud!(NodeY {}, "node");
crud!(EdgeY {}, "edge");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeY {
    /// 自增主键
    pub id: i32,

    /// 源节点VID
    #[serde(default)]
    pub vid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeY {
    /// 自增主键
    pub id: i32,

    /// 源节点VID
    #[serde(default)]
    pub src_vid: String,

    /// 目标节点VID
    #[serde(default)]
    pub dst_vid: String,
}
