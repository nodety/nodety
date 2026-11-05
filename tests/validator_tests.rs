use crate::common::{expr, graph, sig_u};
use assert_matches::assert_matches;
use nodety::{
    Edge, Nodety,
    demo_type::DemoType,
    inference::{InferenceConfig, Scopes},
    scope::LocalParamID,
    type_expr::{ScopedTypeRef, TypeExpr},
    validation::{GraphLocation, ValidationError, ValidationErrorKind},
};
use petgraph::graph::NodeIndex;
use std::collections::BTreeMap;

mod common;

///                  <T extends Comparable>
///  |    int| ----- |T                   |
#[test]
pub fn test_validate_bounds() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("<T extends Comparable>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));

    assert_eq!(errors, []);
}

///                  <T extends Countable>
///  |    int| ----- |T                  |
#[test]
pub fn test_validate_invalid_bounds() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("<T extends Countable>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());
    let errors = engine.validate(&scopes);

    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::TypeParameterBoundsExceeded(_), .. });
}

/// Don't delete this test! It tests, that type params can only get inferred when their bounds are inferred.
/// More info in Candidates::pick_best`.
///
///                  <T, U extends T>
///  |    int| ----- |T             |
///  | String| ----- |U             |
#[test]
pub fn test_validate_invalid_bounds_complex() {
    let engine = graph(
        vec![sig_u("() -> (Integer, String)"), sig_u("<T, U extends T>(T, U) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    // U is not inferred before its bound T: String exceeds the inferred T = Integer.
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::TypeParameterBoundsExceeded(_), .. });
}

#[test]
pub fn test_validate_edge_directions_valid() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("(Comparable) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);

    // invalid
    let engine = graph(vec![sig_u("() -> (Comparable)"), sig_u("(Integer) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::TypeMismatch(_), .. });
}

///                      <T, U extends Array<T>>
///  |        int| ----- |T                    |
///  | Array<int>| ----- |U                    |
#[test]
pub fn test_validate_bounds_complex() {
    let engine = graph(
        vec![sig_u("() -> (Integer, Array<Integer>)"), sig_u("<T, U extends Array<T>>(T, U) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

///     <T>       <T>
///  |   T| ----- |T   |
#[test]
pub fn test_validate_identity() {
    let engine = graph(vec![sig_u("<T>() -> (T)"), sig_u("<T>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

///  |int  |
#[test]
pub fn test_validate_edge_missing() {
    let engine = graph(vec![sig_u("(Integer) -> ()")], vec![]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::EdgeMissingOnInput, .. });
}

///                              <T, U extends keyof T>
///  |   {a: Any, b: Any}| ----- |T                   |
///  |               "a" | ----- |U                   |
#[test]
pub fn test_valid_keyof() {
    let engine = graph(
        vec![sig_u("() -> ({a: Any, b: Any}, 'a')"), sig_u("<T, U extends keyof T>(T, U) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

///                              <T, U extends keyof T>
///  |   {a: Any, b: Any}| ----- |T                   |
///  |           "a"|"b" | ----- |U                   |
#[test]
pub fn test_valid_keyof_union() {
    let engine = graph(
        vec![sig_u("() -> ({a: Any, b: Any}, 'a'|'b')"), sig_u("<#0, #1 extends keyof #0>(#0, #1) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    // let scope = scopes.get(&NodeIndex::from(1)).unwrap();

    assert_eq!(errors, []);
}

///                              <T, U extends keyof T>
///  |   {a: Any, b: Any}| ----- |T                   |
///  |               "c" | ----- |U                   |
#[test]
pub fn test_invalid_keyof() {
    let engine = graph(
        vec![sig_u("() -> ({a: Any, b: Any}, 'c')"), sig_u("<T, U extends keyof T>(T, U) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    let errors = engine.validate(&scopes);
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::TypeParameterBoundsExceeded(_), .. });
}

#[test]
pub fn test_invalid_type() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("(String) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::TypeMismatch(_), .. });
}

#[test]
pub fn test_stuff() {
    let engine = graph(vec![sig_u("<T>() -> (T)"), sig_u("(Array<{a: Integer}>) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

/// | String default: "str" |
#[test]
pub fn test_default_types_type() {
    let engine = graph(vec![sig_u("(String = 'str') -> ()")], vec![]);
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

#[test]
fn test_validate_index_type() {
    let engine = graph(
        vec![sig_u("() -> ({a: Integer}, 'b')"), sig_u("<#0, #1, #2 extends #0[#1]>(#0, #1) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

#[test]
fn test_validate_intersection() {
    let engine = graph(
        vec![
            sig_u("() -> ({a: Integer}, {b: Float})"),
            sig_u("<#0, #1, #2 extends #0 & #1>(#0, #1) -> (#2)"),
            sig_u("({a: Integer, b: Float}) -> ()"),
        ],
        vec![(0, 1, 0, 0), (0, 1, 1, 1), (1, 2, 0, 0)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

#[test]
fn test_validate_intersection_invalid() {
    let engine = graph(
        vec![
            sig_u("() -> ({a: Integer}, {a: Integer})"),
            sig_u("<#0, #1, #2 extends #0 & #1>(#0, #1) -> (#2)"),
            sig_u("({a: Float}) -> ()"),
        ],
        vec![(0, 1, 0, 0), (0, 1, 1, 1), (1, 2, 0, 0)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));

    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::InsufficientlyInferredTypes, .. });
}

#[test]
fn test_validate_non_unit() {
    let engine = graph(
        vec![sig_u("() -> (Integer|Unit)"), sig_u("<T>(T) -> (T extends Unit ? Never : T)"), sig_u("(Integer) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

#[test]
fn test_validate_generic_closure() {
    let engine = graph(
        vec![
            sig_u("() -> (Array<{}>)"),
            sig_u("<#0 extends Any -> Never>() -> (#0)"),
            sig_u("<#1>(#1, (#1[Integer]) -> (Boolean)) -> ()"),
        ],
        vec![(0, 2, 0, 0), (1, 2, 0, 1)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));

    assert_eq!(errors, []);
}

#[test]
fn test_runtime_panicked() {
    let engine = graph(
        vec![
            sig_u("() -> (Array<{'false start': Boolean}>)"),
            sig_u("<I extends {}>(Array<I>) -> (I & {place: Integer|Unit, competitor: Integer})"),
            sig_u("<T, K extends keyof T>(T, K = 'place') -> (T[K])"),
        ],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));

    assert_eq!(errors, []);
}

#[test]
fn test_validate_multiple_edges_on_one_input() {
    let mut nodety = Nodety::<DemoType>::new();
    let a = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    let b = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    let c = nodety.add_node(sig_u("(Integer) -> ()")).unwrap();
    nodety.add_edge(a, c, Edge { source_port: 0, target_port: 0 }).unwrap();
    nodety.add_edge(b, c, Edge { source_port: 0, target_port: 0 }).unwrap();

    let errors = nodety.validate(&nodety.infer(&InferenceConfig::default()));
    assert!(errors.iter().any(|e| matches!(e.kind, ValidationErrorKind::MultipleEdgesOnOneInput)));
}

#[test]
fn test_validate_empty_scopes() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("(Integer) -> ()")], vec![(0, 1, 0, 0)]);
    let empty_scopes: Scopes<DemoType> = BTreeMap::new();
    let errors = engine.validate(&empty_scopes);
    assert!(!errors.is_empty(), "validation with empty scopes should produce errors");
    assert!(errors.iter().any(|e| matches!(e.kind, ValidationErrorKind::InsufficientlyInferredTypes)));
}

/// Cyclic graph with generic inference.
/// Source seeds both cycle nodes on port 0; T infers to Integer in both.
/// (Flow collection visits the source first, so 0->1 and 0->2 are collected before the cycle.)
#[test]
fn test_inference_and_validation_cyclic_graph() {
    let engine = graph(
        vec![sig_u("() -> (Integer)"), sig_u("<T>(T, T) -> (T)"), sig_u("<T>(T, T) -> (T)")],
        vec![(0, 1, 0, 0), (0, 2, 0, 0), (2, 1, 0, 1), (1, 2, 0, 1)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    let errors = engine.validate(&scopes);
    assert!(errors.is_empty(), "cyclic graph should infer and validate: {:?}", errors);

    let inferred_t = TypeExpr::<DemoType, ScopedTypeRef<DemoType>>::param_with_infer(LocalParamID::from("T"), true)
        .normalize(scopes.get(&NodeIndex::from(1)).unwrap());
    assert_eq!(expr("Integer"), inferred_t, "T in cycle node A should infer to Integer");

    let inferred_t_b = TypeExpr::<DemoType, ScopedTypeRef<DemoType>>::param_with_infer(LocalParamID::from("T"), true)
        .normalize(scopes.get(&NodeIndex::from(2)).unwrap());
    assert_eq!(expr("Integer"), inferred_t_b, "T in cycle node B should infer to Integer");
}

#[test]
fn default_key_can_use_a_generic_records_guaranteed_field() {
    use nodety::Node;

    let mut engine = Nodety::<DemoType>::new();
    let root = engine.add_node(sig_u("<A extends {time: Integer}>() -> ()")).unwrap();
    let source = engine
        .add_node(Node { signature: sig_u("() -> (A)"), parent: Some(root), type_hints: Default::default() })
        .unwrap();
    let mut signature = sig_u("<C extends {}>(C, {property: keyof C | 'place'}) -> ()");
    signature.default_input_types.insert(1, "{property: 'time'}".parse().unwrap());
    let sort = engine.add_node(Node { signature, parent: Some(root), type_hints: Default::default() }).unwrap();
    engine.add_edge(source, sort, Edge { source_port: 0, target_port: 0 }).unwrap();
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));
    assert_eq!(errors, []);
}

///                                     <A extends Comparable | String, B extends Comparable | String>
///  | Integer | Unit| ----------------- |A                                                           |
///  |        Integer| ----------------- |B                                                           |
#[test]
fn test_optional_into_ordering_comparison_exceeds_bound() {
    let engine = graph(
        vec![
            sig_u("() -> (Integer | Unit, Integer)"),
            sig_u("<A extends Comparable | String, B extends Comparable | String>(A, B) -> (Boolean)"),
        ],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(&InferenceConfig::default()));

    let [
        ValidationError { location: GraphLocation::Edge(0), kind: ValidationErrorKind::TypeParameterBoundsExceeded(d) },
    ] = errors.as_slice()
    else {
        panic!("expected the bound of A to be exceeded at edge 0, got {errors:?}");
    };
    // The outermost layer has the bound as parent and the arm of the candidate outside of it as child.
    let outer = d.layers().last().unwrap();
    assert_eq!(outer.parent, expr("Comparable | String").into_erased_scope_refs());
    assert_eq!(outer.child, expr("Unit").into_erased_scope_refs());
}

/// A parameter without a flow to infer it from stays insufficiently inferred, also when it has a bound.
///                  <T extends Comparable>
///  |      <U>() -> (U)| ----- |T          |
#[test]
fn test_uninferred_source_into_bounded_param_is_insufficiently_inferred() {
    let engine = graph(vec![sig_u("<U>() -> (U)"), sig_u("<T extends Comparable>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());
    let errors = engine.validate(&scopes);
    assert!(
        errors.iter().all(|e| !matches!(e.kind, ValidationErrorKind::TypeParameterBoundsExceeded(_))),
        "{errors:?}"
    );
}

/// A generic closure as the default of a closure input: its parameter is inferred from the inputs the input gives it.
///                          <C>
///  | {a: Boolean} | ----- |C                                                   |
///                         |(C) -> (Integer) = <P extends {a: Comparable}>(P) -> (Integer) |
#[test]
fn a_generic_closure_default_whose_inputs_exceed_its_bound() {
    use nodety::Node;

    let errors = closure_default_errors("{a: Boolean}");
    let [ValidationError { location, kind: ValidationErrorKind::TypeParameterBoundsExceeded(diagnostics) }] =
        &errors[..]
    else {
        panic!("Expected the bound to be exceeded, got {errors:?}");
    };
    assert_eq!(*location, GraphLocation::InputPort { node_idx: 1, input_idx: 1 });
    assert_matches!(diagnostics.layers()[0].child, TypeExpr::Type(DemoType::Boolean));
    assert_eq!(closure_default_errors("{a: Integer}"), []);

    fn closure_default_errors(row: &str) -> Vec<ValidationError<DemoType>> {
        let mut engine = graph(vec![sig_u(&format!("() -> ({row})"))], vec![]);
        let mut signature = sig_u("<C>(C, (C) -> (Integer)) -> ()");
        signature.default_input_types.insert(1, "<P extends {a: Comparable}>(P) -> (Integer)".parse().unwrap());
        let node = engine.add_node(Node { signature, parent: None, type_hints: Default::default() }).unwrap();
        engine.add_edge(NodeIndex::from(0), node, Edge { source_port: 0, target_port: 0 }).unwrap();
        engine.validate(&engine.infer(&InferenceConfig::default()))
    }
}
