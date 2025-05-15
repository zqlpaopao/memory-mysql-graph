use std::ops::Sub;
use std::sync::LazyLock;
use petgraph::algo::astar;
use petgraph::graph::NodeIndex;
use petgraph::prelude::EdgeRef;
use rbatis::{crud, table_sync, RBatis};
use rbatis::rbdc::pool::Pool;
use rbdc_mysql::MysqlDriver;
use serde::{Deserialize, Serialize};
use memory_mysql_graph::controller::graph::GraphInstance;
use memory_mysql_graph::controller::graph::Node as N;
use memory_mysql_graph::controller::graph::Edge as E;
use anyhow::Result;
use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // write_data().await;
    read_data().await;

}



pub async fn test_axum() -> Result<()> {
    let app = routes();
    // .layer(middleware::from_fn(middleware_log::logging_middleware));

    println!("axum Listening on 0.0.0.0:3000");

    let listener = TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await?;
    Ok(())
}

pub fn routes() -> axum::Router {
    Router::new().route("/", get(index))
}

async fn index() -> &'static str {
    "Hello world!"
}



/***************************************** 初始化数据 *********************************/
pub static GRAPH: LazyLock<GraphInstance> = LazyLock::new(|| GraphInstance::new(100000));

async fn read_data() {
    let rb = init_db().await;
    // 处理节点 (如果 add_node 返回 Future)
    let nodes = Node::select_all(&rb).await;
    if let Ok(node_batch) = &nodes {
        for v in node_batch {
            // 如果 add_node 是异步操作，需要 await
            GRAPH.add_node(N {
                vid: v.vid.to_string(),
                data: Default::default()
            }).await.unwrap(); // 假设 add_node 返回 Future
        }
    }

    // 处理边 (顺序执行)
    let edges = Edge::select_all(&rb).await;
    if let Ok(edge_batch) = &edges{
        for v in edge_batch {
            GRAPH.add_edge(
                v.src_vid.as_str(),
                v.dst_vid.as_str(),
                E {
                    weight: v.id as usize,
                    src_vid: v.src_vid.to_string(),
                    dst_vid: v.dst_vid.to_string(),
                    data: Default::default(),
                }
            ).await.unwrap();
        }
    }

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
        |e| 0,             // 边权重
        |_| 0                        // 启发式函数（设为 0 等价于 Dijkstra）
        // |node| manhattan_distance(&graph[node], &graph[target_node]).try_into().unwrap(), //曼哈顿算法
    );
    let read =  t.elapsed();
    // std::process::exit(0);

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
                    println!("  {} -> {} (权重: {:?})",
                             graph[src].vid,
                             graph[dst].vid,
                             edge.weight().weight);
                    break;
                }
            }
        }
    } else {
        println!("无法从 Router 到达 Server");
    }
    println!("代码执行时间: {:.2?}",read); // 输出: 代码执行时间: 2.00s

    println!("代码执行时间: {:.2?}", t1.elapsed()); // 输出: 代码执行时间: 2.00s

    // println!("{:?}", GRAPH);
}




/***************************************** 写数据 *********************************/

async fn write_data(){

    let rb = init_db().await;

    // let mut node = Vec::with_capacity(100000);
    // let mut edge = Vec::with_capacity(100000);

    for i in  0..100000{
        Node::insert(&rb, &Node { id: 0, vid: i.to_string() }).await.unwrap();
        if i > 0{
            // edge.push(Edge{
            //     id: 0,
            //     src_vid: (i-1).to_string(),
            //     dst_vid: i.to_string(),
            // });
            Edge::insert(&rb,&Edge{
                id: 0,
                src_vid: (i-1).to_string(),
                dst_vid: i.to_string(),
            }).await.unwrap();
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

crud!(Node {});
crud!(Edge {});

#[derive(Debug, Clone,Serialize,Deserialize)]
pub struct Node {
    /// 自增主键
    pub id: i32,

    /// 源节点VID
    #[serde(default)]
    pub vid: String,

}

#[derive(Debug, Clone,Serialize,Deserialize)]
pub struct Edge {
    /// 自增主键
    pub id: i32,

    /// 源节点VID
    #[serde(default)]
    pub src_vid: String,

    /// 目标节点VID
    #[serde(default)]
    pub dst_vid: String,
}