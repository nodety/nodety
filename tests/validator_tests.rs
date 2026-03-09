use crate::common::{graph, sig_u};
use assert_matches::assert_matches;
use maplit::hashset;
use nodety::{
    Nodety,
    demo_type::DemoType,
    inference::InferenceConfig,
    validation::{ValidationError, ValidationErrorKind},
};

mod common;

///                  <T extends Comparable>
///  |    int| ----- |T                   |
#[test]
pub fn test_validate_bounds() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("<T extends Comparable>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));

    assert_eq!(errors, []);
}

///                  <T extends Countable>
///  |    int| ----- |T                  |
#[test]
pub fn test_validate_invalid_bounds() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("<T extends Countable>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(InferenceConfig::default());
    let errors = engine.validate(&scopes);

    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::InsufficientlyInferredTypes, .. });
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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::InsufficientlyInferredTypes, .. });
}

#[test]
pub fn test_validate_edge_directions_valid() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("(Comparable) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_eq!(errors, []);

    // invalid
    let engine = graph(vec![sig_u("() -> (Comparable)"), sig_u("(Integer) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_eq!(errors, []);
}

///     <T>       <T>
///  |   T| ----- |T   |
#[test]
pub fn test_validate_identity() {
    let engine = graph(vec![sig_u("<T>() -> (T)"), sig_u("<T>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_eq!(errors, []);
}

///      |  int| ----- |int   int| ----- |int             int|
///tags: |1,2  |       |1,2      |       |1,2 (required 1, 2)|
#[test]
pub fn test_validate_tags() {
    let engine = graph(
        vec![
            sig_u("() -> (Integer)").with_tags(hashset! {1,2}),
            sig_u("(Integer) -> (Integer)").with_tags(hashset! {1,2}),
            sig_u("(Integer) -> ()").with_required_tags(hashset! {1,2}),
        ],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_eq!(errors, []);
}

///      |  int| ----- |int   int| ----- |int                |
///tags: |1    |       |1,2      |       |1,2 (required 1, 2)|
#[test]
pub fn test_invalidate_tags() {
    let engine = graph(
        vec![
            sig_u("() -> (Integer)").with_tags(hashset! {1}),
            sig_u("(Integer) -> (Integer)").with_tags(hashset! {1,2}),
            sig_u("(Integer) -> ()").with_required_tags(hashset! {1,2}),
        ],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_matches!(&errors[0], ValidationError { kind: ValidationErrorKind::TagMissing(tags), .. } if tags == &hashset! {2});
}

///  |int  |
#[test]
pub fn test_validate_edge_missing() {
    let engine = graph(vec![sig_u("(Integer) -> ()")], vec![]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
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
    let scopes = engine.infer(InferenceConfig::default());
    let errors = engine.validate(&scopes);
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::InsufficientlyInferredTypes, .. });
}

#[test]
pub fn test_invalid_type() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("(String) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::TypeMismatch(_), .. });
}

#[test]
pub fn test_stuff() {
    let engine = graph(vec![sig_u("<T>() -> (T)"), sig_u("(Array<{a: Integer}>) -> ()")], vec![(0, 1, 0, 0)]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_eq!(errors, []);
}

/// | String default: "str" |
#[test]
pub fn test_default_types_type() {
    let engine = graph(vec![sig_u("(String = 'str') -> ()")], vec![]);
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
    assert_eq!(errors, []);
}

#[test]
fn test_validate_index_type() {
    let engine = graph(
        vec![sig_u("() -> ({a: Integer}, 'b')"), sig_u("<#0, #1, #2 extends #0[#1]>(#0, #1) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));

    assert_matches!(errors[0], ValidationError { kind: ValidationErrorKind::InsufficientlyInferredTypes, .. });
}

#[test]
fn test_validate_non_unit() {
    let engine = graph(
        vec![sig_u("() -> (Integer|Unit)"), sig_u("<T>(T) -> (T extends Unit ? Never : T)"), sig_u("(Integer) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));
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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));

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
    let errors = engine.validate(&engine.infer(InferenceConfig::default()));

    assert_eq!(errors, []);
}

#[test]
fn test_validate_multiple_edges_on_one_input() {
    let mut nodety = Nodety::<DemoType>::new();
    let a = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    let b = nodety.add_node(sig_u("() -> (Integer)")).unwrap();
    let c = nodety.add_node(sig_u("(Integer) -> ()")).unwrap();
    nodety.add_edge(a, c, 0, 0);
    nodety.add_edge(b, c, 0, 0);

    let errors = nodety.validate(&nodety.infer(InferenceConfig::default()));
    assert!(errors.iter().any(|e| matches!(e.kind, ValidationErrorKind::MultipleEdgesOnOneInput)));
}
