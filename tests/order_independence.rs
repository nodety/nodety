//! Inference gives the same result for every order in which the nodes were added.
use crate::common::sig_u;
use nodety::{Edge, Node, Nodety, demo_type::DemoType, inference::InferenceConfig};
use proptest::prelude::*;

mod common;

/// A node: its signature and the index of its parent in the spec.
type NodeSpec = (&'static str, Option<usize>);

/// The inferred value of every parameter of every node (by spec index) and the number of validation errors, for the
/// nodes added in `order` (a permutation of the spec indices that adds parents before their children).
fn infer_in_order(nodes: &[NodeSpec], edges: &[(usize, usize, usize)], order: &[usize]) -> (Vec<String>, usize) {
    let mut nodety = Nodety::<DemoType>::new();
    let mut idx = vec![None; nodes.len()];
    for &k in order {
        let (signature, parent) = nodes[k];
        let node = match parent {
            Some(parent) => Node::new_child(sig_u(signature), idx[parent].expect("parents come first")),
            None => Node::new(sig_u(signature)),
        };
        idx[k] = Some(nodety.add_node(node).unwrap());
    }
    let idx: Vec<_> = idx.into_iter().map(|i| i.expect("every node is added")).collect();
    for &(source, target, target_port) in edges {
        nodety.add_edge(idx[source], idx[target], Edge { source_port: 0, target_port }).unwrap();
    }
    let scopes = nodety.infer(&InferenceConfig::default());
    let mut inferred = Vec::new();
    for (k, (signature, _)) in nodes.iter().enumerate() {
        for param in sig_u(signature).parameters.keys() {
            let value = scopes[&idx[k]].lookup_inferred(param).map(|(t, s)| t.normalize(&s));
            let value = value.map_or("uninferred".to_string(), |t| t.force_remove_scope_portals().to_string());
            inferred.push(format!("{k}.{param:?} = {value}"));
        }
    }
    (inferred, nodety.validate(&scopes).len())
}

/// Every permutation of `0..n` that adds parents before their children.
fn valid_orders(nodes: &[NodeSpec]) -> Vec<Vec<usize>> {
    fn permutations(n: usize) -> Vec<Vec<usize>> {
        if n == 0 {
            return vec![vec![]];
        }
        let mut out = Vec::new();
        for p in permutations(n - 1) {
            for i in 0..=p.len() {
                let mut q = p.clone();
                q.insert(i, n - 1);
                out.push(q);
            }
        }
        out
    }
    let parents_first = |order: &Vec<usize>| {
        let position = |k: usize| order.iter().position(|&x| x == k);
        nodes.iter().enumerate().all(|(k, (_, parent))| parent.is_none_or(|parent| position(parent) < position(k)))
    };
    permutations(nodes.len()).into_iter().filter(parents_first).collect()
}

fn assert_order_independent(nodes: &[NodeSpec], edges: &[(usize, usize, usize)]) -> (Vec<String>, usize) {
    let canonical: Vec<usize> = (0..nodes.len()).collect();
    let expected = infer_in_order(nodes, edges, &canonical);
    for order in valid_orders(nodes) {
        assert_eq!(infer_in_order(nodes, edges, &order), expected, "order {order:?}");
    }
    expected
}

/// A source of `'a' | Unit`, a node with children whose output drops `Unit`, and a sink of `'a'`.
const FILTER: [NodeSpec; 5] = [
    ("() -> (Array<'a' | Unit>)", None),
    ("<I, O>(Array<I>) -> (Array<O>)", None),
    ("(Array<'a'>) -> ()", None),
    ("() -> (I, Integer)", Some(1)),
    ("(O | Unit) -> ()", Some(1)),
];
const FILTER_EDGES: [(usize, usize, usize); 3] = [(0, 1, 0), (3, 4, 0), (1, 2, 0)];

#[test]
fn a_union_covered_by_a_concrete_arm_infers_the_same_in_every_order() {
    let (inferred, errors) = assert_order_independent(&FILTER, &FILTER_EDGES);
    assert!(inferred.iter().any(|v| v.starts_with("1.") && v.ends_with("= \"a\"")), "O = 'a': {inferred:?}");
    assert_eq!(errors, 0);
}

#[test]
fn a_chain_through_a_generic_node_infers_the_same_in_every_order() {
    let nodes: [NodeSpec; 6] = [
        ("() -> (Array<'a' | 'b' | Unit>)", None),
        ("<X>(X) -> (X)", None),
        ("<I, O>(Array<I>) -> (Array<O>)", None),
        ("(Array<'a' | 'b'>) -> ()", None),
        ("() -> (I, Integer)", Some(2)),
        ("(O | Unit) -> ()", Some(2)),
    ];
    let edges = [(0, 1, 0), (1, 2, 0), (4, 5, 0), (2, 3, 0)];
    let (_, errors) = assert_order_independent(&nodes, &edges);
    assert_eq!(errors, 0);
}

proptest! {
    /// Random insertion orders of a larger graph: two filters in a row, fed through a generic node.
    #[test]
    fn random_insertion_orders_infer_the_same(seed in proptest::collection::vec(any::<u32>(), 9)) {
        let nodes: [NodeSpec; 9] = [
            ("() -> (Array<'a' | Unit>)", None),
            ("<X>(X) -> (X)", None),
            ("<I, O>(Array<I>) -> (Array<O>)", None),
            ("() -> (I, Integer)", Some(2)),
            ("(O | Unit) -> ()", Some(2)),
            ("<I, O>(Array<I>) -> (Array<O>)", None),
            ("() -> (I, Integer)", Some(5)),
            ("(O | Unit) -> ()", Some(5)),
            ("(Array<'a'>) -> ()", None),
        ];
        let edges = [(0, 1, 0), (1, 2, 0), (3, 4, 0), (2, 5, 0), (6, 7, 0), (5, 8, 0)];
        // A random order, then parents moved in front of their children.
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by_key(|&k| seed[k]);
        let mut valid = Vec::new();
        for k in order {
            if let Some(parent) = nodes[k].1 && !valid.contains(&parent) {
                valid.push(parent);
            }
            if !valid.contains(&k) {
                valid.push(k);
            }
        }
        let canonical: Vec<usize> = (0..nodes.len()).collect();
        prop_assert_eq!(infer_in_order(&nodes, &edges, &valid), infer_in_order(&nodes, &edges, &canonical));
    }
}
