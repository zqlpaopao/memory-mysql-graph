// use memory_mysql_graph::controller::graph::{Edge, GraphInstance, Node};
// use std::collections::HashMap;
// use std::sync::Arc;
// use std::time::Duration;
// #[tokio::main]
// async fn main() {
//     // 添加节点
//     // test_node_lifecycle().await;
//
//     // 测试边的更新和写入
//     // test_edge_lifecycle().await;
//
//     // 批量添加节点
//     // test_batch_operations().await;
//
//     // 并发读取
//     test_concurrent_access().await;
//
//     tokio::time::sleep(Duration::from_secs(100)).await;
// }
//
// // 并发读取
// async fn test_concurrent_access() {
//     let graph = Arc::new(GraphInstance::new(1000)); // 增大缓存容量
//
//     // 并发写入测试
//     let write_handles: Vec<_> = (0..10000)
//         .map(|i| {
//             let graph = graph.clone();
//             tokio::spawn(async move {
//                 // 混合操作：添加节点、边、更新数据
//                 let vid = format!("node{}", i);
//                 graph
//                     .add_node(Node {
//                         vid: vid.clone(),
//                         data: HashMap::new(),
//                     })
//                     .await
//                     .unwrap();
//
//                 if i > 0 {
//                     let prev_vid = format!("node{}", i - 1);
//                     graph
//                         .add_edge(
//                             prev_vid.clone(),
//                             vid.clone(),
//                             Edge {
//                                 src_vid: prev_vid,
//                                 dst_vid: vid.clone(),
//                                 data: [("weight".to_string(), i.to_string())].into(),
//                             },
//                         )
//                         .await
//                         .unwrap();
//
//                     // 并发更新
//                     graph
//                         .update_node(vid.clone(), [("index".to_string(), i.to_string())].into())
//                         .await
//                         .unwrap();
//                 }
//
//                 // 返回节点ID用于验证
//                 vid
//             })
//         })
//         .collect();
//
//     // 并发读取测试（与写入同时进行）
//     let read_handle = tokio::spawn({
//         let graph = graph.clone();
//         async move {
//             let mut success_count = 0;
//             for _ in 0..5000 {
//                 let node_count = graph.get_all_nodes().len();
//                 let edge_count = graph.get_all_edges().len();
//                 if node_count > 0 || edge_count > 0 {
//                     success_count += 1;
//                 }
//                 tokio::time::sleep(std::time::Duration::from_millis(10)).await;
//             }
//             success_count
//         }
//     });
//
//     // 等待所有写入完成
//     let mut node_vids = Vec::new();
//     for handle in write_handles {
//         node_vids.push(handle.await.unwrap());
//     }
//
//     // 验证写入结果
//     println!("{} {}", graph.get_all_nodes().len(), 100);
//     println!("{} {}", graph.get_all_edges().len(), 99); // 99条边
//
//     // 验证读取结果
//     let read_success = read_handle.await.unwrap();
//     println!("{} {}", read_success >= 30, "并发读取成功率应大于60%");
//
//     // 混合读写测试
//     let mixed_handles: Vec<_> = node_vids
//         .into_iter()
//         .enumerate()
//         .map(|(i, vid)| {
//             let graph = graph.clone();
//             tokio::spawn(async move {
//                 // 读取
//                 let node = graph.get_node(&vid).unwrap();
//                 // 写入
//                 if i % 2 == 0 {
//                     graph
//                         .update_node(vid, [("modified".to_string(), "true".to_string())].into())
//                         .await
//                         .unwrap();
//                 }
//                 node
//             })
//         })
//         .collect();
//
//     for handle in mixed_handles {
//         println!("{}", handle.await.unwrap().vid.starts_with("node"));
//     }
// }
//
// // 批量添加节点
// async fn test_batch_operations() {
//     let graph = GraphInstance::new(100);
//     let nodes = vec![
//         create_test_node("n1"),
//         create_test_node("n2"),
//         create_test_node("n3"),
//     ];
//
//     // 批量添加节点
//     let indices = graph.add_nodes(nodes).await.unwrap();
//     assert_eq!(indices.len(), 3);
//
//     // 批量添加边
//     let edges = vec![
//         (
//             "n1".to_string(),
//             "n2".to_string(),
//             create_test_edge("n1", "n2"),
//         ),
//         (
//             "n2".to_string(),
//             "n3".to_string(),
//             create_test_edge("n2", "n3"),
//         ),
//     ];
//     let edge_indices = graph.add_edges(edges).await.unwrap();
//     println!("{:#?}", graph.get_graph_snapshot());
//     assert_eq!(edge_indices.len(), 2);
// }
//
// // 测试边的更新和写入
// async fn test_edge_lifecycle() {
//     let graph = GraphInstance::new(100);
//     graph.add_node(create_test_node("n1")).await.unwrap();
//     graph.add_node(create_test_node("n2")).await.unwrap();
//     let edge = create_test_edge("n1", "n2");
//
//     // 测试添加
//     let idx = graph
//         .add_edge("n1".to_string(), "n2".to_string(), edge.clone())
//         .await
//         .unwrap();
//     println!("{}", idx.index() >= 0);
//
//     let res = graph.get_edges_between("n1", "n2");
//     println!("{:?}", res);
//
//     let edges = graph.get_all_edges();
//     println!("{:?}", edges);
//
//     // 测试查询
//     assert_eq!(graph.get_edges_between("n1", "n2").len(), 1);
//     assert_eq!(graph.get_out_edges("n1").len(), 1);
//     assert_eq!(graph.get_in_edges("n2").len(), 1);
//
//     // 测试更新
//     graph
//         .update_edge_by_vids(
//             "n1".to_string(),
//             "n2".to_string(),
//             [("weight".to_string(), "5".to_string())].into(),
//         )
//         .await
//         .unwrap();
//     assert_eq!(graph.get_edges_between("n1", "n2")[0].data["weight"], "5");
//
//     // 测试删除
//     graph
//         .remove_edge_by_vids("n1".to_string(), "n2".to_string())
//         .await
//         .unwrap();
//     assert!(graph.get_edges_between("n1", "n2").is_empty());
// }
//
// // 添加节点
// async fn test_node_lifecycle() {
//     let graph = GraphInstance::new(100);
//     let node = create_test_node("n1");
//
//     // 测试添加
//     let idx = graph.add_node(node.clone()).await.unwrap();
//     println!("n1 : index {:?}", idx);
//
//     // 测试查询
//     // let res = graph.get_node("n1").unwrap().vid;
//     println!("n1 : info {:?}", idx);
//
//     println!(
//         "n1 : graph.get_all_nodes().len() {:?}",
//         graph.get_all_nodes().len()
//     );
//
//     // 测试更新
//     graph
//         .update_node(
//             "n1".to_string(),
//             [("new".to_string(), "data".to_string())].into(),
//         )
//         .await
//         .unwrap();
//
//     let res = graph.get_node("n1").unwrap();
//     println!("n1 : info {:?}", res);
//
//     // 测试删除
//     graph.remove_node("n1".to_string()).await.unwrap();
//     let res = graph.get_node("n1");
//     println!("n1 : info {:?}", res);
// }
//
// // 测试工具函数
// fn create_test_node(vid: &str) -> Node {
//     Node {
//         vid: vid.to_string(),
//         data: [("type".to_string(), "test".to_string())].into(),
//     }
// }
//
// fn create_test_edge(from: &str, to: &str) -> Edge {
//     Edge {
//         src_vid: from.to_string(),
//         dst_vid: to.to_string(),
//         data: [("relation".to_string(), "connected".to_string())].into(),
//     }
// }

fn main() {}
