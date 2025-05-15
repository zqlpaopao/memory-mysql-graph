use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::{Directed, EdgeType, Graph, Undirected};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct NodeM {
    pub vid: String,
    pub data: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct EdgeM {
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

pub enum GraphType {
    Directed(Graph<NodeM, EdgeM, Directed>),
    Undirected(Graph<NodeM, EdgeM, Undirected>),
}

// 明确泛型参数为自定义的 Node 和 Edge 类型
pub struct GraphInstance<Ty: EdgeType>
where
    Ty: EdgeType,
{
    segment: String,
    pub inner: Arc<RwLock<Graph<NodeM, EdgeM, Ty>>>, // 使用自定义 Node/Edge
    node_cache: Arc<RwLock<HashMap<String, NodeIndex>>>, // 带 Ix 参数的索引
    edge_cache: Arc<RwLock<HashMap<String, EdgeIndex>>>, // 带 Ix 参数的索引
}

impl<Ty: EdgeType> GraphInstance<Ty> {
    pub fn new(node_cache: usize, edge_cache: usize, graph: Graph<NodeM, EdgeM, Ty>) -> Self {
        Self {
            segment: "_".to_owned(),
            inner: Arc::new(RwLock::new(graph)),
            node_cache: Arc::new(RwLock::new(HashMap::with_capacity(node_cache))),
            edge_cache: Arc::new(RwLock::new(HashMap::with_capacity(edge_cache))),
        }
    }

    pub async fn add_node(&self, node: NodeM) -> Result<NodeIndex, GraphError> {
        let vid = node.vid.clone();
        {
            let cache = self.node_cache.read().await;
            if cache.contains_key(&vid) {
                return Err(GraphError::NodeExistsError(vid));
            }
        }
        let mut graph = self.inner.write().await;
        let mut cache = self.node_cache.write().await;

        if cache.contains_key(&vid) {
            return Err(GraphError::NodeExistsError(vid));
        }

        let index = graph.add_node(node);
        cache.insert(vid, index);
        Ok(index)
    }

    pub async fn find_node_by_vid(&self, vid: &str) -> Option<NodeM>
    where
        NodeM: Clone,
    {
        let index = {
            let cache = self.node_cache.read().await;
            cache.get(vid).cloned()
        };

        if let Some(idx) = index {
            let graph = self.inner.read().await;
            graph.node_weight(idx).cloned()
        } else {
            None
        }
    }

    pub async fn remove_node(&self, vid: &str) -> bool {
        let index = {
            let cache = self.node_cache.read().await;
            cache.get(vid).cloned()
        };

        if let Some(idx) = index {
            let mut graph = self.inner.write().await;
            graph.remove_node(idx);
            true
        } else {
            false
        }
    }

    pub async fn update_node(&self, vid: &str, data: HashMap<String, String>) -> bool
    where
        NodeM: Clone,
    {
        let index = {
            let cache = self.node_cache.read().await;
            cache.get(vid).cloned()
        };

        if let Some(idx) = index {
            let mut graph = self.inner.write().await;
            if let Some(node) = graph.node_weight_mut(idx) {
                node.data = data;
                return true;
            }
        }
        false
    }

    pub async fn add_edge(
        &self,
        from_vid: &str,
        to_vid: &str,
        edge: EdgeM,
    ) -> Result<EdgeIndex, GraphError> {
        let edge_key = format!("{}{}{}", from_vid, self.segment, to_vid);

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

        let index = {
            let mut graph = self.inner.write().await;
            let mut edge_cache = self.edge_cache.write().await;

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
        edge_vid: &str,
        data: HashMap<String, String>,
    ) -> Result<(), GraphError> {
        let edge_idx = {
            let cache = self.edge_cache.read().await;
            cache.get(edge_vid).copied()
        };

        let Some(edge_idx) = edge_idx else {
            return Err(GraphError::EdgeNotFound(edge_vid.into()));
        };

        {
            let mut graph = self.inner.write().await;
            if let Some(edge) = graph.edge_weight_mut(edge_idx) {
                edge.data.extend(data);
                return Ok(());
            }
        }

        Err(GraphError::EdgeNotFound(edge_vid.into()))
    }

    pub async fn update_edge_by_node(
        &self,
        from_vid: &str,
        to_vid: &str,
        data: HashMap<String, String>,
    ) -> Result<(), GraphError> {
        let edge_key = format!("{}{}{}", from_vid, self.segment, to_vid);

        let edge_idx = {
            let cache = self.edge_cache.read().await;
            cache.get(&edge_key).copied()
        };

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

    pub async fn remove_edge(&self, edge_vid: String) -> bool {
        let edge_idx = {
            let cache = self.edge_cache.read().await;
            cache.get(&edge_vid).copied()
        };

        if let Some(idx) = edge_idx {
            let mut graph = self.inner.write().await;
            graph.remove_edge(idx);
            true
        } else {
            false
        }
    }

    pub async fn remove_edge_by_node(&self, from_vid: &str, to_vid: &str) -> bool {
        let edge_key = format!("{}{}{}", from_vid, self.segment, to_vid);

        let edge_idx = {
            let cache = self.edge_cache.read().await;
            cache.get(&edge_key).copied()
        };

        if let Some(idx) = edge_idx {
            let mut graph = self.inner.write().await;
            graph.remove_edge(idx);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_node(vid: &str) -> NodeM {
        NodeM {
            vid: vid.to_string(),
            data: HashMap::new(),
        }
    }

    fn create_test_edge(from: &str, to: &str) -> EdgeM {
        EdgeM {
            weight: 1,
            src_vid: from.to_string(),
            dst_vid: to.to_string(),
            data: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_node_crud() {
        let graph = GraphInstance::new(10, 10, Graph::new());
        let node = create_test_node("test_node");

        let index = graph.add_node(node.clone()).await.unwrap();
        assert!(index.index() >= 0);

        let found = graph.find_node_by_vid("test_node").await.unwrap();
        assert_eq!(found.vid, "test_node");

        let mut update_data = HashMap::new();
        update_data.insert("key".to_string(), "value".to_string());
        assert!(graph.update_node("test_node", update_data.clone()).await);
        assert_eq!(
            graph.find_node_by_vid("test_node").await.unwrap().data["key"],
            "value"
        );

        assert!(graph.remove_node("test_node").await);
        assert!(graph.find_node_by_vid("test_node").await.is_none());
    }

    #[tokio::test]
    async fn test_edge_crud() {
        let graph = GraphInstance::new(10, 10, Graph::new());
        graph.add_node(create_test_node("n1")).await.unwrap();
        graph.add_node(create_test_node("n2")).await.unwrap();

        let edge = create_test_edge("n1", "n2");
        let edge_idx = graph.add_edge("n1", "n2", edge).await.unwrap();
        assert!(edge_idx.index() >= 0);

        let edge_key = format!("n1{}n2", graph.segment);
        assert!(
            graph
                .update_edge(&edge_key, [("weight".to_string(), "10".to_string())].into())
                .await
                .is_ok()
        );

        assert!(
            graph
                .update_edge_by_node(
                    "n1",
                    "n2",
                    [("type".to_string(), "friend".to_string())].into()
                )
                .await
                .is_ok()
        );

        assert!(graph.remove_edge(edge_key).await);
        assert!(
            graph
                .update_edge_by_node("n1", "n2", HashMap::new())
                .await
                .is_err()
        );
    }
}
