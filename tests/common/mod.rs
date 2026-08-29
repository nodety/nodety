use nodety::Nodety;
use nodety::demo_type::DemoType;
use nodety::nodety::Edge;
use nodety::type_expr::{ParamRef, ScopedTypeExpr, ScopedTypeRef, TypeExpr, node_signature::NodeSignature};
use std::str::FromStr;

#[allow(dead_code)]
pub fn graph(nodes: Vec<NodeSignature<DemoType>>, edges: Vec<(usize, usize, usize, usize)>) -> Nodety<DemoType> {
    let mut nodety = Nodety::new();
    let mut node_ids = Vec::new();
    for node in nodes {
        node_ids.push(nodety.add_node(node).unwrap());
    }
    for (source, target, source_port, target_port) in edges {
        nodety.add_edge(node_ids[source], node_ids[target], Edge { source_port, target_port }).unwrap();
    }
    nodety
}

#[allow(dead_code)]
#[track_caller]
pub fn sig(input: &str) -> NodeSignature<DemoType, ScopedTypeRef<DemoType>> {
    NodeSignature::from_str(input).expect(&format!("Failed to parse {input}"))
}

#[allow(dead_code)]
#[track_caller]
pub fn sig_u(input: &str) -> NodeSignature<DemoType, ParamRef> {
    NodeSignature::from_str(input).expect(&format!("Failed to parse {input}"))
}

#[allow(dead_code)]
#[track_caller]
pub fn expr(input: &str) -> ScopedTypeExpr<DemoType> {
    TypeExpr::from_str(input).expect(&format!("Failed to parse {input}"))
}

#[allow(dead_code)]
#[track_caller]
pub fn expr_u(input: &str) -> TypeExpr<DemoType, ParamRef> {
    TypeExpr::from_str(input).expect(&format!("Failed to parse {input}"))
}
