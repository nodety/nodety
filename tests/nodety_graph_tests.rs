use crate::common::{graph, sig_u};
use assert_matches::assert_matches;
use maplit::btreemap;
use nodety::{
    Node, NodeSignature, Nodety, demo_type::DemoType, inference::InferenceConfig, nodety::NodetyError,
    scope::LocalParamID, type_expr::TypeExpr,
};
use petgraph::graph::NodeIndex;

mod common;

#[test]
fn test_parent_child_tracking() {
    let mut system = Nodety::<DemoType>::new();

    // Add root node (no parent)
    let root = system.add_node(sig_u("<T>() -> ()")).unwrap();

    // Add child with existing parent
    let child = system.add_node(Node::new_child(sig_u("(T) -> ()"), root)).unwrap();

    // Cannot remove root while it has children
    assert!(matches!(system.remove_node(root), Err(NodetyError::NodeHasChildren)));

    // Can remove leaf (child has no children)
    assert!(system.remove_node(child).is_ok());
    assert!(system.remove_node(root).is_ok());
}

#[test]
fn test_parent_must_exist() {
    let mut system = Nodety::<DemoType>::new();
    let _root = system.add_node(NodeSignature::default()).unwrap();

    // NodeIndex::from(99) is a non-existent parent
    let bad_node = Node::new_child(sig_u("() -> ()"), NodeIndex::from(99));
    assert!(matches!(system.add_node(bad_node), Err(NodetyError::ParentNotFound)));
}

#[test]
fn test_no_cycle_on_update() {
    let mut system = Nodety::<DemoType>::new();
    let a = system.add_node(NodeSignature::default()).unwrap();
    let b = system.add_node(Node::new_child(NodeSignature::default(), a)).unwrap();

    // Making A's parent B would create cycle A -> B -> A
    assert!(matches!(
        system.update_node(a, Node::new_child(NodeSignature::default(), b)),
        Err(NodetyError::CycleDetected)
    ));
}

#[test]
fn test_nodety_default() {
    let nodety = Nodety::<DemoType>::default();
    assert_eq!(nodety.program().node_count(), 0);
}

#[test]
fn test_nodety_with_capacity() {
    let nodety = Nodety::<DemoType>::with_capacity(10, 20);
    assert_eq!(nodety.program().node_count(), 0);
}

#[test]
fn test_get_node() {
    let mut nodety = Nodety::<DemoType>::new();
    let idx = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    assert!(nodety.get_node(idx).is_some());
    assert!(nodety.get_node(NodeIndex::from(99)).is_none());
}

#[test]
fn test_update_node_success() {
    let mut nodety = Nodety::<DemoType>::new();
    let idx = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    assert!(nodety.update_node(idx, sig_u("() -> (String)")).is_ok());
}

#[test]
fn test_update_node_not_found() {
    let mut nodety = Nodety::<DemoType>::new();
    let result = nodety.update_node(NodeIndex::from(42), sig_u("() -> ()"));
    assert_matches!(result, Err(nodety::nodety::NodetyError::NodeNotFound));
}

#[test]
fn test_update_node_parent_not_found() {
    let mut nodety = Nodety::<DemoType>::new();
    let idx = nodety.add_node(sig_u("() -> ()")).unwrap();
    let update = Node::new_child(sig_u("() -> ()"), NodeIndex::from(99));
    let result = nodety.update_node(idx, update);
    assert_matches!(result, Err(nodety::nodety::NodetyError::ParentNotFound));
}

#[test]
fn test_update_node_cycle_detection() {
    let mut nodety = Nodety::<DemoType>::new();
    let a = nodety.add_node(NodeSignature::default()).unwrap();
    let b = nodety.add_node(Node::new_child(NodeSignature::default(), a)).unwrap();
    let c = nodety.add_node(Node::new_child(NodeSignature::default(), b)).unwrap();

    let update = Node::new_child(NodeSignature::default(), c);
    let result = nodety.update_node(a, update);
    assert_matches!(result, Err(nodety::nodety::NodetyError::CycleDetected));
}

#[test]
fn test_update_node_reparent() {
    let mut nodety = Nodety::<DemoType>::new();
    let a = nodety.add_node(NodeSignature::default()).unwrap();
    let b = nodety.add_node(NodeSignature::default()).unwrap();
    let child = nodety.add_node(Node::new_child(NodeSignature::default(), a)).unwrap();

    assert!(nodety.update_node(child, Node::new_child(NodeSignature::default(), b)).is_ok());
    assert!(nodety.remove_node(a).is_ok());
}

#[test]
fn test_remove_node_with_children_fails() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(NodeSignature::default()).unwrap();
    let _child = nodety.add_node(Node::new_child(NodeSignature::default(), parent)).unwrap();

    assert_matches!(nodety.remove_node(parent), Err(nodety::nodety::NodetyError::NodeHasChildren));
}

#[test]
fn test_remove_node_nonexistent_is_ok() {
    let mut nodety = Nodety::<DemoType>::new();
    assert!(nodety.remove_node(NodeIndex::from(99)).is_ok());
}

#[test]
fn test_add_remove_edge() {
    let mut nodety = Nodety::<DemoType>::new();
    let a = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    let b = nodety.add_node(sig_u("(Integer) -> ()")).unwrap();

    let edge = nodety.add_edge(a, b, 0, 0);
    assert!(nodety.remove_edge(edge).is_some());
    assert!(nodety.remove_edge(edge).is_none());
}

#[test]
fn test_to_dot() {
    let mut nodety = Nodety::<DemoType>::new();
    let a = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    let b = nodety.add_node(sig_u("(Integer) -> ()")).unwrap();
    nodety.add_edge(a, b, 0, 0);

    let dot = nodety.to_dot();
    assert!(dot.contains("digraph"), "Expected 'digraph' in DOT output: {dot}");
}

#[test]
fn test_nodety_error_display() {
    let err = nodety::nodety::NodetyError::CycleDetected;
    assert!(format!("{}", err).contains("CycleDetected"));

    let err = nodety::nodety::NodetyError::ParentNotFound;
    assert!(format!("{}", err).contains("ParentNotFound"));
}

#[test]
fn test_node_default() {
    let node = Node::<DemoType>::default();
    assert!(node.parent.is_none());
    assert!(node.type_hints.is_empty());
}

#[test]
fn test_node_with_type_hints() {
    let node = Node::<DemoType>::new(sig_u("() -> ()"));
    let hints = btreemap! {
        LocalParamID::from('T') => TypeExpr::Type(DemoType::Integer)
    };
    let node = node.with_type_hints(hints.clone());
    assert_eq!(node.type_hints, hints);
}

#[test]
fn test_node_into_node_from_sig() {
    use nodety::nodety::IntoNode;
    let sig = sig_u("(Integer) -> (String)");
    let node = sig.into_node();
    assert!(node.parent.is_none());
}

#[test]
fn test_validate_no_errors_on_valid_graph() {
    let engine = graph(
        vec![sig_u("() -> (Integer, String)"), sig_u("(Integer, String) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert!(errors.is_empty());
}

#[test]
fn test_validate_generic_closure_with_edges() {
    let engine = graph(vec![sig_u("() -> (Array<Integer>)"), sig_u("<T>(Array<T>) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert!(errors.is_empty());
}
