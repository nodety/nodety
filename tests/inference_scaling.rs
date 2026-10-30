//! How inference time grows with the depth of the graph: a chain of n generic nodes vs. n parallel ones.
use nodety::{
    Nodety, demo_type::DemoType, inference::InferenceConfig, nodety::Edge, type_expr::node_signature::NodeSignature,
};
use std::str::FromStr;
use std::time::Instant;

fn sig(s: &str) -> NodeSignature<DemoType> {
    NodeSignature::from_str(s).unwrap()
}

fn chain(n: usize) -> Nodety<DemoType> {
    let mut nodety = Nodety::new();
    let mut prev = nodety.add_node(sig("() -> ({a: Integer, b: String})")).unwrap();
    for _ in 0..n {
        let node = nodety.add_node(sig("<X extends {}>(X) -> (X)")).unwrap();
        nodety.add_edge(prev, node, Edge { source_port: 0, target_port: 0 }).unwrap();
        prev = node;
    }
    nodety
}

fn parallel(n: usize) -> Nodety<DemoType> {
    let mut nodety = Nodety::new();
    let src = nodety.add_node(sig("() -> ({a: Integer, b: String})")).unwrap();
    for _ in 0..n {
        let node = nodety.add_node(sig("<X extends {}>(X) -> (X)")).unwrap();
        nodety.add_edge(src, node, Edge { source_port: 0, target_port: 0 }).unwrap();
    }
    nodety
}

/// Inferred types used to stay references to the type of the node before, so a type n nodes deep resolved through all
/// n scopes on every access: a chain of 800 took 3.4 s to infer and 3.0 s to validate in release (cubic in depth).
#[test]
fn test_a_deep_chain_infers_in_linear_time() {
    let graph = chain(800);
    let start = Instant::now();
    let scopes = graph.infer(&InferenceConfig::default());
    assert_eq!(graph.validate(&scopes), []);
    // About 10 ms in release, about 0.2 s in debug.
    assert!(start.elapsed().as_secs() < 3, "{:?}", start.elapsed());
}

#[test]
#[ignore = "cargo test --release --test inference_scaling -- --ignored --nocapture"]
fn scaling() {
    for n in [50, 100, 200, 400, 800] {
        for (name, graph) in [("chain", chain(n)), ("parallel", parallel(n))] {
            let start = Instant::now();
            let scopes = graph.infer(&InferenceConfig::default());
            let infer = start.elapsed();
            let start = Instant::now();
            let errors = graph.validate(&scopes);
            println!("{name:>8} n={n:>4}: infer {infer:?}, validate {:?}, errors {}", start.elapsed(), errors.len());
        }
    }
}
