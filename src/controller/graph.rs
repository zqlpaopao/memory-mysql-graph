use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::Graph;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Node {
    pub vid: String,
    pub data: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub weight: usize,
    pub src_vid: String,
    pub dst_vid: String,
    pub data: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Node with VID '{0}' already exists")]
    NodeExistsError(String),
    #[error("Node '{0}' not found")]
    NodeNotFound(String),

    #[error("Edge between '{0}' and '{1}' already exists")]
    EdgeExists(String, String),
    #[error("Edge '{0}' not found")]
    EdgeNotFound(String),
}

// 全局图实例
// 全局图实例

// 封装图实例，提供线程安全的访问
#[derive(Debug)]
pub struct GraphInstance {
    segment: String,
    pub inner: Arc<RwLock<Graph<Node, Edge>>>,
    node_cache: Arc<RwLock<HashMap<String, NodeIndex>>>,
    edge_cache: Arc<RwLock<HashMap<String, EdgeIndex>>>,
}

impl GraphInstance {
    pub fn new(cache_size: usize) -> Self {
        Self {
            segment: "_".to_owned(),
            inner: Arc::new(RwLock::new(Graph::new())),
            node_cache: Arc::new(RwLock::new(HashMap::with_capacity(cache_size))),
            edge_cache: Arc::new(RwLock::new(HashMap::with_capacity(cache_size))),
        }
    }

    // 添加节点
    pub async fn add_node(&self, node: Node) -> Result<NodeIndex, GraphError> {
        let vid = node.vid.clone();
        {
            let cache = self.node_cache.read().await;
            if cache.contains_key(&vid) {
                return Err(GraphError::NodeExistsError(vid));
            }
        }
        let mut graph = self.inner.write().await;
        let mut cache = self.node_cache.write().await;

        // 双重检查（避免在获取写锁前其他线程已添加该节点）
        if cache.contains_key(&vid) {
            return Err(GraphError::NodeExistsError(vid));
        }

        // 添加节点并更新缓存
        let index = graph.add_node(node);
        cache.insert(vid, index);
        Ok(index)
    }

    // 根据vid查找节点
    pub async fn find_node_by_vid(&self, vid: &str) -> Option<Node> {
        let index = {
            let cache = self.node_cache.read().await;
            cache.get(vid).cloned() // 克隆 NodeIndex，得到 Option<NodeIndex>
        };

        if let Some(idx) = index {
            let graph = self.inner.read().await;
            graph.node_weight(idx).cloned()
        } else {
            None
        }
    }

    // 删除节点
    pub async fn remove_node(&self, vid: &str) -> bool {
        let index = {
            let cache = self.node_cache.read().await;
            cache.get(vid).cloned() // 克隆 NodeIndex，得到 Option<NodeIndex>
        };

        if let Some(idx) = index {
            let mut graph = self.inner.write().await;
            graph.remove_node(idx);
            true
        } else {
            false
        }
    }

    // 更新节点数据
    pub async fn update_node(&self, vid: &str, data: HashMap<String, String>) -> bool {
        let index = {
            let cache = self.node_cache.read().await;
            cache.get(vid).cloned() // 克隆 NodeIndex，得到 Option<NodeIndex>
        };

        if let Some(idx) = index {
            let mut graph = self.inner.write().await;
            graph[idx].data = data;
            true
        } else {
            false
        }
    }

    // 添加边
    pub async fn add_edge(
        &self,
        from_vid: &str,
        to_vid: &str,
        edge: Edge,
    ) -> Result<EdgeIndex, GraphError> {
        let edge_key = format!("{}{}{}", from_vid, self.segment, to_vid);

        // 阶段1：共享锁快速检查
        let (edge_exists, from_index, to_index) = {
            let edge_cache = self.edge_cache.read().await;
            let node_cache = self.node_cache.read().await;
            (
                edge_cache.contains_key(&edge_key),
                node_cache.get(from_vid).cloned(),
                node_cache.get(to_vid).cloned(),
            )
        };

        if edge_exists {
            return Err(GraphError::EdgeExists(from_vid.into(), to_vid.into()));
        }
        if from_index.is_none() || to_index.is_none() {
            let missing = if from_index.is_none() {
                from_vid
            } else {
                to_vid
            };
            return Err(GraphError::NodeNotFound(missing.into()));
        }

        // 阶段2：排他锁写入
        let index = {
            let mut graph = self.inner.write().await;
            let mut edge_cache = self.edge_cache.write().await;

            // 最终一致性检查
            if edge_cache.contains_key(&edge_key) {
                return Err(GraphError::EdgeExists(from_vid.into(), to_vid.into()));
            }

            let index = graph.add_edge(from_index.unwrap(), to_index.unwrap(), edge);
            edge_cache.insert(edge_key, index);
            index
        };

        Ok(index)
    }

    pub async fn update_edge(
        &self,
        edge_vid: &str, // 明确使用边唯一ID
        data: HashMap<String, String>,
    ) -> Result<(), GraphError> {
        // 阶段1：快速查找（读锁）
        let edge_idx = {
            let cache = self.edge_cache.read().await;
            cache.get(edge_vid).copied()
        };

        let Some(edge_idx) = edge_idx else {
            return Err(GraphError::EdgeNotFound(edge_vid.into()));
        };

        // 阶段2：精准更新（写锁）
        {
            let mut graph = self.inner.write().await;
            if let Some(edge) = graph.edge_weight_mut(edge_idx) {
                edge.data.extend(data); // 合并数据而非覆盖
                return Ok(());
            }
        }

        Err(GraphError::EdgeNotFound(edge_vid.into()))
    }

    // 更新边数据
    pub async fn update_edge_by_node(
        &self,
        from_vid: &str,
        to_vid: &str,
        data: HashMap<String, String>,
    ) -> Result<(), GraphError> {
        let edge_key = format!("{}{}{}", from_vid, self.segment, to_vid);

        // 阶段1：查找边索引（读锁）
        let edge_idx = {
            let cache = self.edge_cache.read().await;
            cache.get(&edge_key).copied()
        };

        // 阶段2：更新数据（写锁）
        if let Some(idx) = edge_idx {
            let mut graph = self.inner.write().await;
            if let Some(edge) = graph.edge_weight_mut(idx) {
                edge.data.extend(data);
                return Ok(());
            }
        }

        Err(GraphError::EdgeNotFound(format!(
            "{}->{}",
            from_vid, to_vid
        )))
    }

    // 删除边
    pub async fn remove_edge(&self, edge_vid: String) -> bool {
        // 阶段1：查找边索引（读锁）
        let edge_idx = {
            let cache = self.edge_cache.read().await;
            cache.get(&edge_vid).copied()
        };

        // 阶段2：更新数据（写锁）
        if let Some(idx) = edge_idx {
            let mut graph = self.inner.write().await;
            graph.remove_edge(idx);
            true
        } else {
            false
        }
    }
    pub async fn add_edge_by_source(&self){

    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Barrier;
    use std::sync::Arc;

    pub static GRAPH: LazyLock<GraphInstance> = LazyLock::new(|| GraphInstance::new(0));

    // 测试工具函数
    fn create_test_node(vid: &str) -> Node {
        Node {
            vid: vid.to_string(),
            data: HashMap::new(),
        }
    }

    fn create_test_edge(from: &str, to: &str) -> Edge {
        Edge {
            weight: 1,
            src_vid: from.to_string(),
            dst_vid: to.to_string(),
            data: [("relation".to_string(), "connected".to_string())].into(),
        }
    }

    #[tokio::test]
    async fn test_node_crud() {
        // 测试节点全生命周期
        let node = create_test_node("test_node");

        // 添加
        let index = GRAPH.add_node(node.clone()).await.unwrap();
        assert!(index.index() >= 0);

        // 查询
        let found = GRAPH.find_node_by_vid("test_node").await.unwrap();
        assert_eq!(found.vid, "test_node");

        // 更新
        let mut update_data = HashMap::new();
        update_data.insert("key".to_string(), "value".to_string());
        assert!(GRAPH.update_node("test_node", update_data.clone()).await);
        assert_eq!(GRAPH.find_node_by_vid("test_node").await.unwrap().data["key"], "value");

        // 删除
        assert!(GRAPH.remove_node("test_node").await);
        let found = GRAPH.find_node_by_vid("test_node").await;
        println!("found: {:?}", found);
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_edge_crud() {
        // 准备节点
        GRAPH.add_node(create_test_node("n1")).await.unwrap();
        GRAPH.add_node(create_test_node("n2")).await.unwrap();

        // 添加边
        let edge = create_test_edge("n1", "n2");
        let edge_idx = GRAPH.add_edge("n1", "n2", edge).await.unwrap();
        assert!(edge_idx.index() >= 0);

        // 查询边
        let edge_key = format!("n1{}n2", GRAPH.segment);
        assert!(GRAPH.update_edge(&edge_key, [("weight".to_string(), "10".to_string())].into()).await.is_ok());

        // 通过端点查询
        assert!(GRAPH.update_edge_by_node("n1", "n2", [("type".to_string(), "friend".to_string())].into()).await.is_ok());

        // 删除边
        assert!(GRAPH.remove_edge(edge_key).await);
        assert!(GRAPH.update_edge_by_node("n1", "n2", HashMap::new()).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_node_operations() {
        let barrier = Arc::new(Barrier::new(1000));
        let mut handles = vec![];

        for i in 0..1000 {
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                let vid = format!("concurrent_{}", i);

                // 并发添加
                GRAPH.add_node(create_test_node(&vid)).await.unwrap();

                // 并发更新
                GRAPH.update_node(&vid, [("thread".to_string(), i.to_string())].into()).await;

                // 验证
                assert_eq!(GRAPH.find_node_by_vid(&vid).await.unwrap().data["thread"], i.to_string());
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_edge_operations() {
        // 准备节点
        GRAPH.add_node(create_test_node("src")).await.unwrap();
        GRAPH.add_node(create_test_node("dst")).await.unwrap();

        let barrier = Arc::new(Barrier::new(4));
        let mut handles = vec![];

        for i in 0..4 {
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                let edge_key = format!("src{}dst", GRAPH.segment);

                // 测试边添加幂等性
                match GRAPH.add_edge("src", "dst", create_test_edge("src", "dst")).await {
                    Ok(_) | Err(GraphError::EdgeExists(_, _)) => (),
                    Err(e) => panic!("Unexpected error: {}", e),
                }

                // 并发更新
                GRAPH.update_edge(&edge_key, [("version".to_string(), i.to_string())].into()).await.unwrap();

                // 验证至少有一个更新生效
                let edge = GRAPH.update_edge(&edge_key, HashMap::new()).await;
                assert!(edge.is_ok());
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_error_handling() {
        // 测试重复节点
        GRAPH.add_node(create_test_node("dup")).await.unwrap();
        let result = GRAPH.add_node(create_test_node("dup")).await;
        assert!(matches!(result, Err(GraphError::NodeExistsError(_))));

        // 测试不存在的节点
        assert!(!GRAPH.update_node("nonexistent", HashMap::new()).await);

        // 测试不存在的边
        let edge_key = "invalid_edge".to_string();
        assert!(matches!(
            GRAPH.update_edge(&edge_key, HashMap::new()).await,
            Err(GraphError::EdgeNotFound(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_mixed_concurrent_operations() {
        let test_iters = 100;
        let mut handles = vec![];

        for i in 0..test_iters {
            handles.push(tokio::spawn(async move {
                let vid = format!("mixed_{}", i);

                // 混合操作
                GRAPH.add_node(create_test_node(&vid)).await.unwrap();
                GRAPH.update_node(&vid, [("index".to_string(), i.to_string())].into()).await;

                if i > 0 {
                    let prev_vid = format!("mixed_{}", i-1);
                    let _ = GRAPH.add_edge(&prev_vid, &vid, create_test_edge(&prev_vid, &vid)).await;

                    // 随机读取验证
                    if i % 10 == 0 {
                        assert!(GRAPH.find_node_by_vid(&vid).await.is_some());
                    }
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 最终一致性验证
        assert_eq!(
            GRAPH.find_node_by_vid(&format!("mixed_{}", test_iters-1)).await.unwrap().data["index"],
            (test_iters-1).to_string()
        );
    }
}
