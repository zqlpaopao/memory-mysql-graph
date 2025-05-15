// use arc_swap::ArcSwap;
// use crossbeam::channel::{unbounded, Sender};
// use dashmap::DashMap;
// use petgraph::graph::{EdgeIndex, NodeIndex};
// use petgraph::Graph;
// use std::collections::HashMap;
// use std::sync::Arc;
// use thiserror::Error;
// use tokio::sync::oneshot;
//
// // 数据结构定义
// #[derive(Debug, Clone)]
// pub struct Node {
//     pub vid: String,
//     pub data: HashMap<String, String>,
// }
//
// #[derive(Debug, Clone)]
// pub struct Edge {
//     pub src_vid: String,
//     pub dst_vid: String,
//     pub data: HashMap<String, String>,
// }
//
// // 错误类型
// #[derive(Debug, Error)]
// pub enum GraphError {
//     #[error("Node with VID '{0}' already exists")]
//     NodeExistsError(String),
//     #[error("Node '{0}' not found")]
//     NodeNotFound(String),
//     #[error("Edge between '{0}' and '{1}' already exists")]
//     EdgeExists(String, String),
//     #[error("Edge '{0}' not found")]
//     EdgeNotFound(String),
//     #[error("Cannot remove node '{0}' because it still has edges")]
//     NodeHasEdges(String),
//     #[error("Internal channel error")]
//     ChannelError,
// }
//
// // 写操作枚举
// enum WriteOp {
//     AddNode(Node, oneshot::Sender<Result<NodeIndex, GraphError>>),
//     UpdateNode {
//         vid: String,
//         data: HashMap<String, String>,
//         resp: oneshot::Sender<Result<(), GraphError>>,
//     },
//     RemoveNode {
//         vid: String,
//         resp: oneshot::Sender<Result<(), GraphError>>,
//     },
//
//     //edge
//     AddEdge {
//         from_vid: String,
//         to_vid: String,
//         edge: Edge,
//         resp: oneshot::Sender<Result<EdgeIndex, GraphError>>,
//     },
//     UpdateEdge {
//         key: String,
//         data: HashMap<String, String>,
//         resp: oneshot::Sender<Result<(), GraphError>>,
//     },
//
//     RemoveEdgeByKey {
//         key: String,
//         resp: oneshot::Sender<Result<(), GraphError>>,
//     },
//     RemoveEdgeByVids {
//         from_vid: String,
//         to_vid: String,
//         resp: oneshot::Sender<Result<(), GraphError>>,
//     },
//     UpdateEdgeByVids {
//         from_vid: String,
//         to_vid: String,
//         data: HashMap<String, String>,
//         resp: oneshot::Sender<Result<(), GraphError>>,
//     },
//     AddNodes(
//         Vec<Node>,
//         oneshot::Sender<Result<Vec<NodeIndex>, GraphError>>,
//     ),
//     AddEdges(
//         Vec<(String, String, Edge)>,
//         oneshot::Sender<Result<Vec<EdgeIndex>, GraphError>>,
//     ),
//
//     Shutdown,
// }
//
// // 图实例结构体
// pub struct GraphInstance {
//     segment: String,
//     inner: Arc<ArcSwap<Graph<Node, Edge>>>,
//     node_cache: Arc<DashMap<String, NodeIndex>>,
//     edge_cache: Arc<DashMap<String, EdgeIndex>>,
//     write_sender: Sender<WriteOp>,
// }
//
// impl GraphInstance {
//     pub fn new(cache_size: usize) -> Self {
//         let (tx, rx) = unbounded();
//         let inner = Arc::new(ArcSwap::new(Arc::new(Graph::new())));
//         let node_cache = Arc::new(DashMap::with_capacity(cache_size));
//         let edge_cache = Arc::new(DashMap::with_capacity(cache_size));
//
//         // 启动后台写线程
//         let inner_clone = inner.clone();
//         let node_cache_clone = node_cache.clone();
//         let edge_cache_clone = edge_cache.clone();
//         std::thread::spawn(move || {
//             Self::write_processor(rx, inner_clone, node_cache_clone, edge_cache_clone)
//         });
//
//         Self {
//             segment: "_".to_owned(),
//             inner,
//             node_cache,
//             edge_cache,
//             write_sender: tx,
//         }
//     }
//
//     fn write_processor(
//         rx: crossbeam::channel::Receiver<WriteOp>,
//         inner: Arc<ArcSwap<Graph<Node, Edge>>>,
//         node_cache: Arc<DashMap<String, NodeIndex>>,
//         edge_cache: Arc<DashMap<String, EdgeIndex>>,
//     ) {
//         while let Ok(op) = rx.recv() {
//             match op {
//                 WriteOp::AddNode(node, resp) => {
//                     let vid = node.vid.clone();
//                     if node_cache.contains_key(&vid) {
//                         let _ = resp.send(Err(GraphError::NodeExistsError(vid)));
//                         continue;
//                     }
//
//                     // 克隆当前图并修改
//                     let mut new_graph = (**inner.load()).clone();
//                     let index = new_graph.add_node(node);
//
//                     // 更新缓存
//                     node_cache.insert(vid, index);
//
//                     // 发布新图
//                     inner.store(Arc::new(new_graph));
//                     let _ = resp.send(Ok(index));
//                 }
//
//                 WriteOp::AddEdge { from_vid, to_vid, edge, resp } => {
//                     // 1. 快速检查（无锁）
//                     let edge_key = format!("{}_{}", from_vid, to_vid);
//                     if edge_cache.contains_key(&edge_key) {
//                         let _ = resp.send(Err(GraphError::EdgeExists(from_vid, to_vid)));
//                         return;
//                     }
//
//                     // 2. 获取节点索引（共享读锁）
//                     let (from_idx, to_idx) = {
//                         let from = node_cache.get(&from_vid).map(|e| *e.value());
//                         let to = node_cache.get(&to_vid).map(|e| *e.value());
//                         match (from, to) {
//                             (Some(f), Some(t)) => (f, t),
//                             (None, _) => {
//                                 let _ = resp.send(Err(GraphError::NodeNotFound(from_vid)));
//                                 return;
//                             }
//                             (_, None) => {
//                                 let _ = resp.send(Err(GraphError::NodeNotFound(to_vid)));
//                                 return;
//                             }
//                         }
//                     };
//
//                     // 3. 最小化写锁范围
//                     let index = {
//                         let mut graph = Arc::make_mut(Arc::get_mut(&mut inner.load_full()).unwrap());
//                         graph.add_edge(from_idx, to_idx, edge)
//                     };
//
//                     // 4. 更新缓存（DashMap自带并发控制）
//                     edge_cache.insert(edge_key.clone(), index);
//                     let _ = resp.send(Ok(index));
//                 }
//
//                 WriteOp::UpdateEdge { key, data, resp } => {
//                     let edge_idx = match edge_cache.get(&key) {
//                         Some(entry) => *entry.value(),
//                         None => {
//                             let _ = resp.send(Err(GraphError::EdgeNotFound(key)));
//                             continue;
//                         }
//                     };
//
//                     // 克隆当前图并修改
//                     let mut new_graph = (**inner.load()).clone();
//                     if let Some(edge) = new_graph.edge_weight_mut(edge_idx) {
//                         edge.data.extend(data);
//                         inner.store(Arc::new(new_graph));
//                         let _ = resp.send(Ok(()));
//                     } else {
//                         let _ = resp.send(Err(GraphError::EdgeNotFound(key)));
//                     }
//                 }
//                 WriteOp::UpdateNode { vid, data, resp } => {
//                     let node_idx = match node_cache.get(&vid) {
//                         Some(entry) => *entry.value(),
//                         None => {
//                             let _ = resp.send(Err(GraphError::NodeNotFound(vid)));
//                             continue;
//                         }
//                     };
//
//                     let mut new_graph = (**inner.load()).clone();
//                     if let Some(node) = new_graph.node_weight_mut(node_idx) {
//                         node.data = data;
//                         inner.store(Arc::new(new_graph));
//                         let _ = resp.send(Ok(()));
//                     } else {
//                         let _ = resp.send(Err(GraphError::NodeNotFound(vid)));
//                     }
//                 }
//
//                 WriteOp::RemoveNode { vid, resp } => {
//                     let node_idx = match node_cache.get(&vid) {
//                         Some(entry) => *entry.value(),
//                         None => {
//                             let _ = resp.send(Err(GraphError::NodeNotFound(vid)));
//                             continue;
//                         }
//                     };
//
//                     let mut new_graph = (**inner.load()).clone();
//                     if new_graph.edges(node_idx).count() > 0 {
//                         let _ = resp.send(Err(GraphError::NodeHasEdges(vid)));
//                         continue;
//                     }
//
//                     new_graph.remove_node(node_idx);
//                     node_cache.remove(&vid);
//                     inner.store(Arc::new(new_graph));
//                     let _ = resp.send(Ok(()));
//                 }
//
//                 WriteOp::RemoveEdgeByKey { key, resp } => {
//                     let edge_idx = match edge_cache.get(&key) {
//                         Some(entry) => *entry.value(),
//                         None => {
//                             let _ = resp.send(Err(GraphError::EdgeNotFound(key)));
//                             continue;
//                         }
//                     };
//
//                     let mut new_graph = (**inner.load()).clone();
//                     if new_graph.remove_edge(edge_idx).is_some() {
//                         edge_cache.remove(&key);
//                         inner.store(Arc::new(new_graph));
//                         let _ = resp.send(Ok(()));
//                     } else {
//                         let _ = resp.send(Err(GraphError::EdgeNotFound(key)));
//                     }
//                 }
//
//                 WriteOp::RemoveEdgeByVids {
//                     from_vid,
//                     to_vid,
//                     resp,
//                 } => {
//                     let edge_key = format!("{}_{}", from_vid, to_vid);
//                     let edge_idx = match edge_cache.get(&edge_key) {
//                         Some(entry) => *entry.value(),
//                         None => {
//                             let _ = resp.send(Err(GraphError::EdgeNotFound(edge_key)));
//                             continue;
//                         }
//                     };
//
//                     let mut new_graph = (**inner.load()).clone();
//                     if new_graph.remove_edge(edge_idx).is_some() {
//                         edge_cache.remove(&edge_key);
//                         inner.store(Arc::new(new_graph));
//                         let _ = resp.send(Ok(()));
//                     } else {
//                         let _ = resp.send(Err(GraphError::EdgeNotFound(edge_key)));
//                     }
//                 }
//
//                 WriteOp::UpdateEdgeByVids {
//                     from_vid,
//                     to_vid,
//                     data,
//                     resp,
//                 } => {
//                     let edge_key = format!("{}_{}", from_vid, to_vid);
//                     let edge_idx = match edge_cache.get(&edge_key) {
//                         Some(entry) => *entry.value(),
//                         None => {
//                             let _ = resp.send(Err(GraphError::EdgeNotFound(edge_key)));
//                             continue;
//                         }
//                     };
//
//                     let mut new_graph = (**inner.load()).clone();
//                     if let Some(edge) = new_graph.edge_weight_mut(edge_idx) {
//                         edge.data.extend(data);
//                         inner.store(Arc::new(new_graph));
//                         let _ = resp.send(Ok(()));
//                     } else {
//                         let _ = resp.send(Err(GraphError::EdgeNotFound(edge_key)));
//                     }
//                 }
//                 WriteOp::AddNodes(nodes, resp) => {
//                     let mut new_graph = (**inner.load()).clone();
//                     let mut indices = Vec::with_capacity(nodes.len());
//                     let mut cache_updates = Vec::with_capacity(nodes.len());
//
//                     for node in nodes {
//                         let vid = node.vid.clone();
//                         if node_cache.contains_key(&vid) {
//                             let _ = resp.send(Err(GraphError::NodeExistsError(vid)));
//                             return;
//                         }
//
//                         let index = new_graph.add_node(node);
//                         indices.push(index);
//                         cache_updates.push((vid, index));
//                     }
//
//                     for (vid, idx) in cache_updates {
//                         node_cache.insert(vid, idx);
//                     }
//
//                     inner.store(Arc::new(new_graph));
//                     let _ = resp.send(Ok(indices));
//                 }
//                 WriteOp::AddEdges(edges, resp) => {
//                     let mut new_graph = (**inner.load()).clone();
//                     let mut edge_indices = Vec::with_capacity(edges.len());
//                     let mut cache_updates = Vec::with_capacity(edges.len());
//
//                     for (from_vid, to_vid, edge) in edges {
//                         let edge_key = format!("{}_{}", from_vid, to_vid);
//                         if edge_cache.contains_key(&edge_key) {
//                             let _ = resp.send(Err(GraphError::EdgeExists(from_vid, to_vid)));
//                             return;
//                         }
//
//                         let from_idx = match node_cache.get(&from_vid) {
//                             Some(entry) => *entry.value(),
//                             None => {
//                                 let _ = resp.send(Err(GraphError::NodeNotFound(from_vid)));
//                                 return;
//                             }
//                         };
//
//                         let to_idx = match node_cache.get(&to_vid) {
//                             Some(entry) => *entry.value(),
//                             None => {
//                                 let _ = resp.send(Err(GraphError::NodeNotFound(to_vid)));
//                                 return;
//                             }
//                         };
//
//                         let index = new_graph.add_edge(from_idx, to_idx, edge);
//                         edge_indices.push(index);
//                         cache_updates.push((edge_key, index));
//                     }
//
//                     for (key, idx) in cache_updates {
//                         edge_cache.insert(key, idx);
//                     }
//
//                     inner.store(Arc::new(new_graph));
//                     let _ = resp.send(Ok(edge_indices));
//                 }
//
//                 WriteOp::Shutdown => break,
//             }
//         }
//     }
//
//     // 批量添加节点
//     pub async fn add_nodes(&self, nodes: Vec<Node>) -> Result<Vec<NodeIndex>, GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::AddNodes(nodes, tx))
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     // 批量添加边
//     pub async fn add_edges(
//         &self,
//         edges: Vec<(String, String, Edge)>,
//     ) -> Result<Vec<EdgeIndex>, GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::AddEdges(edges, tx))
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     // 公共API实现
//     pub async fn add_node(&self, node: Node) -> Result<NodeIndex, GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::AddNode(node, tx))
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     pub async fn add_edge(
//         &self,
//         from_vid: String,
//         to_vid: String,
//         edge: Edge,
//     ) -> Result<EdgeIndex, GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::AddEdge {
//                 from_vid,
//                 to_vid,
//                 edge,
//                 resp: tx,
//             })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     pub async fn update_edge(
//         &self,
//         edge_key: String,
//         data: HashMap<String, String>,
//     ) -> Result<(), GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::UpdateEdge {
//                 key: edge_key,
//                 data,
//                 resp: tx,
//             })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     // 无锁读取方法
//     pub fn get_node(&self, vid: &str) -> Option<Node> {
//         self.node_cache.get(vid).and_then(|entry| {
//             let graph = self.inner.load();
//             graph.node_weight(*entry.value()).cloned()
//         })
//     }
//
//     pub fn get_edge(&self, key: &str) -> Option<Edge> {
//         self.edge_cache.get(key).and_then(|entry| {
//             let graph = self.inner.load();
//             graph.edge_weight(*entry.value()).cloned()
//         })
//     }
//
//     pub fn get_graph_snapshot(&self) -> Arc<Graph<Node, Edge>> {
//         self.inner.load_full()
//     }
//
//     // 获取所有节点（无锁快照）
//     pub fn get_all_nodes(&self) -> Vec<Node> {
//         let graph = self.inner.load();
//         graph
//             .node_indices()
//             .filter_map(|idx| graph.node_weight(idx).cloned())
//             .collect()
//     }
//
//     // 获取节点数据（无锁）
//     pub fn get_node_data(&self, vid: &str) -> Option<HashMap<String, String>> {
//         self.node_cache.get(vid).and_then(|entry| {
//             let graph = self.inner.load();
//             graph.node_weight(*entry.value()).map(|n| n.data.clone())
//         })
//     }
//
//     // 修改节点数据（异步）
//     pub async fn update_node(
//         &self,
//         vid: String,
//         data: HashMap<String, String>,
//     ) -> Result<(), GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::UpdateNode {
//                 vid,
//                 data,
//                 resp: tx,
//             })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     // 删除节点（异步）
//     pub async fn remove_node(&self, vid: String) -> Result<(), GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::RemoveNode { vid, resp: tx })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     /* 边操作 */
//
//     // 获取所有边（无锁快照）
//     pub fn get_all_edges(&self) -> Vec<Edge> {
//         let graph = self.inner.load();
//         graph
//             .edge_indices()
//             .filter_map(|idx| graph.edge_weight(idx).cloned())
//             .collect()
//     }
//
//     // 根据端点获取边（无锁）
//     pub fn get_edges_between(&self, from_vid: &str, to_vid: &str) -> Vec<Edge> {
//         let edge_key_prefix = format!("{}{}{}", from_vid, self.segment, to_vid);
//         let graph = self.inner.load();
//
//         println!("edge_key_prefix: {:?}", self.edge_cache);
//
//         self.edge_cache
//             .iter()
//             .filter(|entry| entry.key().starts_with(&edge_key_prefix))
//             .filter_map(|entry| graph.edge_weight(*entry.value()).cloned())
//             .collect()
//     }
//
//     // 修改边数据（通过边key）
//     pub async fn update_edge_by_key(
//         &self,
//         key: String,
//         data: HashMap<String, String>,
//     ) -> Result<(), GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::UpdateEdge {
//                 key,
//                 data,
//                 resp: tx,
//             })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     // 修改边数据（通过端点vid）
//     pub async fn update_edge_by_vids(
//         &self,
//         from_vid: String,
//         to_vid: String,
//         data: HashMap<String, String>,
//     ) -> Result<(), GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::UpdateEdgeByVids {
//                 from_vid,
//                 to_vid,
//                 data,
//                 resp: tx,
//             })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     // 删除边（通过边key）
//     pub async fn remove_edge_by_key(&self, key: String) -> Result<(), GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::RemoveEdgeByKey { key, resp: tx })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     // 删除边（通过端点vid）
//     pub async fn remove_edge_by_vids(
//         &self,
//         from_vid: String,
//         to_vid: String,
//     ) -> Result<(), GraphError> {
//         let (tx, rx) = oneshot::channel();
//         self.write_sender
//             .send(WriteOp::RemoveEdgeByVids {
//                 from_vid,
//                 to_vid,
//                 resp: tx,
//             })
//             .map_err(|_| GraphError::ChannelError)?;
//         rx.await.map_err(|_| GraphError::ChannelError)?
//     }
//
//     /* 辅助方法 */
//
//     // 获取节点的所有出边（无锁）
//     // 获取节点的所有出边（无锁）
//     pub fn get_out_edges(&self, vid: &str) -> Vec<Edge> {
//         let graph = self.inner.load();
//         self.node_cache
//             .get(vid)
//             .map(|entry| {
//                 graph
//                     .edges(*entry.value())
//                     .map(|edge_ref| edge_ref.weight().clone())
//                     .collect()
//             })
//             .unwrap_or_default()
//     }
//
//     // 获取节点的所有入边（无锁）
//     pub fn get_in_edges(&self, vid: &str) -> Vec<Edge> {
//         let graph = self.inner.load();
//         self.node_cache
//             .get(vid)
//             .map(|entry| {
//                 graph
//                     .edges_directed(*entry.value(), petgraph::Direction::Incoming)
//                     .map(|edge_ref| edge_ref.weight().clone())
//                     .collect()
//             })
//             .unwrap_or_default()
//     }
//
//     pub fn shutdown(&self) {
//         let _ = self.write_sender.send(WriteOp::Shutdown);
//     }
// }
//
// #[cfg(test)]
// mod tests {}
