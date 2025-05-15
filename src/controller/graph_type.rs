// use crate::controller::graph::{EdgeM, NodeM};
// use petgraph::graph::{EdgeIndex, NodeIndex};
// use petgraph::{Directed, Graph, Undirected};
//
// #[macro_export]
// macro_rules! delegate {
//         ($self:ident.$method:ident($($arg:expr),*)) => {
//             match $self {
//                 Self::Directed(g) => g.$method($($arg),*),
//                 Self::Undirected(g) => g.$method($($arg),*),
//             }
//         };
//     }
//
// // -- Graph Type Enum --
// #[derive(Debug)]
// pub enum GraphType {
//     Directed(Graph<NodeM, EdgeM, Directed>),
//     Undirected(Graph<NodeM, EdgeM, Undirected>),
// }
//
// impl GraphType {
//     pub fn add_node(&mut self, node: NodeM) -> NodeIndex {
//         delegate!(self.add_node(node))
//     }
//
//     pub fn node_weight(&self, idx: NodeIndex) -> Option<&NodeM> {
//         delegate!(self.node_weight(idx))
//     }
//
//     pub fn add_edge(&mut self, a: NodeIndex, b: NodeIndex, edge: EdgeM) -> EdgeIndex {
//         delegate!(self.add_edge(a, b, edge))
//     }
//
//     pub fn edge_weight(&self, idx: EdgeIndex) -> Option<&EdgeM> {
//         delegate!(self.edge_weight(idx))
//     }
//
//     pub fn remove_node(&mut self, idx: NodeIndex) -> Option<NodeM> {
//         delegate!(self.remove_node(idx))
//     }
//
//     pub fn remove_edge(&mut self, idx: EdgeIndex) -> Option<EdgeM> {
//         delegate!(self.remove_edge(idx))
//     }
//
//     pub fn edge_weight_mut(&mut self, idx: EdgeIndex) -> Option<&mut EdgeM> {
//         match self {
//             Self::Directed(g) => g.edge_weight_mut(idx),
//             Self::Undirected(g) => g.edge_weight_mut(idx),
//         }
//     }
//
//     /// 获取所有边的集合
//     pub fn all_edges(&self) -> Vec<EdgeM> {
//         match self {
//             GraphType::Directed(g) => g.edge_references()
//                 .map(|er| er.weight().clone())
//                 .collect(),
//             GraphType::Undirected(g) => g.edge_references()
//                 .map(|er| er.weight().clone())
//                 .collect(),
//         }
//     }
//
//     /// 获取特定节点的出边
//     pub fn edges_from(&self, node: NodeIndex) -> Vec<EdgeM> {
//         match self {
//             GraphType::Directed(g) => g.edges(node)
//                 .map(|er| er.weight().clone())
//                 .collect(),
//             GraphType::Undirected(g) => g.edges(node)
//                 .map(|er| er.weight().clone())
//                 .collect(),
//         }
//     }
//
//     /// 获取节点的可变引用
//     pub fn node_weight_mut(&mut self, idx: NodeIndex) -> Option<&mut NodeM> {
//         match self {
//             Self::Directed(g) => g.node_weight_mut(idx),
//             Self::Undirected(g) => g.node_weight_mut(idx),
//         }
//     }
// }
