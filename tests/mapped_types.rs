use crate::common::{expr, expr_u, graph, sig_u};
use nodety::{
    inference::InferenceConfig,
    scope::{LocalParamID, ScopePointer},
    type_expr::subtyping::SupertypeResult,
};
use petgraph::graph::NodeIndex;

mod common;

const WRITE_FIELD: &str =
    "<R extends {}, K extends keyof R, V>(R, K, V) -> ({[P in keyof R]: P extends K ? V : (R[P])})";

#[test]
fn test_format_roundtrip() {
    for notation in [
        "{[K in keyof T]: Array<T[K]>}",
        "{[#1 in keyof {a: Integer}]: #1 extends \"a\" ? Boolean : String}",
        "{[K in keyof T]: Integer} & {a: String}",
    ] {
        assert_eq!(expr_u(notation).to_string(), notation);
    }
}

#[test]
fn test_maps_every_field() {
    assert_eq!(
        expr("{[K in keyof {a: Integer, b: String}]: Array<K>}").normalize_naive(),
        expr("{a: Array<'a'>, b: Array<'b'>}")
    );
}

#[test]
fn test_value_can_read_the_source_field() {
    assert_eq!(
        expr("{[K in keyof {a: Integer, b: String}]: Array<({a: Integer, b: String})[K]>}").normalize_naive(),
        expr("{a: Array<Integer>, b: Array<String>}")
    );
}

#[test]
fn test_replace_one_field() {
    assert_eq!(
        expr("{[K in keyof {a: Integer, b: String}]: K extends 'a' ? Boolean : ({a: Integer, b: String}[K])}")
            .normalize_naive(),
        expr("{a: Boolean, b: String}")
    );
}

/// Fields that the source already has get replaced, the others get added.
#[test]
fn test_override_fields() {
    let base = "{a: Integer, b: String}";
    let overrides = "{b: Boolean, c: Float}";
    let override_expr =
        format!("{{[K in keyof {base}]: K extends keyof {overrides} ? ({overrides}[K]) : ({base}[K])}} & {overrides}");
    assert_eq!(expr(&override_expr).normalize_naive(), expr("{a: Integer, b: Boolean, c: Float}"));
}

#[test]
fn test_distributes_over_unions() {
    assert_eq!(
        expr("{[K in keyof ({a: Integer} | {b: String})]: Boolean}").normalize_naive(),
        expr("{a: Boolean} | {b: Boolean}")
    );
}

#[test]
fn test_keeps_the_inner_type() {
    assert_eq!(expr("{[K in keyof Array<Integer>]: String}").normalize_naive(), expr("Array<String>"));
    assert_eq!(expr("{[K in keyof Integer]: String}").normalize_naive(), expr("Integer"));
    assert_eq!(expr("{[K in keyof Never]: String}").normalize_naive(), expr("Never"));
}

#[test]
fn test_key_param_shadows() {
    let scope = ScopePointer::new(common_scope("<K>"));
    assert_eq!(expr("{[K in keyof {a: Integer}]: K}").normalize(&scope), expr("{a: 'a'}"));
}

#[test]
fn test_keyof_and_index() {
    assert_eq!(expr("keyof {[K in keyof {a: Integer, b: String}]: Boolean}").normalize_naive(), expr("'b' | 'a'"));
    assert_eq!(expr("({[K in keyof {a: Integer, b: String}]: Array<K>})['b']").normalize_naive(), expr("Array<'b'>"));
}

#[test]
fn test_stays_while_the_source_is_unknown() {
    let scope = ScopePointer::new(common_scope("<T>"));
    let mapped = expr("{[K in keyof T]: Array<T[K]>}");
    assert_eq!(mapped.normalize(&scope), mapped);
    assert_eq!(expr("{a: Integer}").supertype_of(&mapped, &scope, &scope), SupertypeResult::Unknown);
    assert_eq!(mapped.supertype_of(&expr("{a: Integer}"), &scope, &scope), SupertypeResult::Unknown);
    assert!(mapped.contains_uninferred(&scope));
    assert!(mapped.validate(&scope).is_ok());
}

#[test]
fn test_subtyping() {
    let mapped = expr("{[K in keyof {a: Integer, b: String}]: Comparable}");
    assert!(mapped.supertype_of_naive(&expr("{a: Integer, b: Float}")).is_supertype());
    assert!(!mapped.supertype_of_naive(&expr("{a: Integer, b: String}")).is_supertype());
    assert!(expr("{a: Comparable}").supertype_of_naive(&mapped).is_supertype());
    assert!(!expr("{a: Integer}").supertype_of_naive(&mapped).is_supertype());
}

///                                <R extends {}, K extends keyof R, V>
///  |  {points: Integer, name: String} | ----- | R                        |
///  |                         'points' | ----- | K       {..R, [K]: V}    | ----- | <X extends {}>X |
///  |                           String | ----- | V                        |
#[test]
fn test_write_field() {
    let engine = graph(
        vec![
            sig_u("() -> ({points: Integer, name: String}, 'points', String)"),
            sig_u(WRITE_FIELD),
            sig_u("<X extends {}>(X) -> ()"),
        ],
        vec![(0, 1, 0, 0), (0, 1, 1, 1), (0, 1, 2, 2), (1, 2, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    assert_eq!(engine.validate(&scopes), []);
    let (x, x_scope) = scopes.get(&NodeIndex::from(2)).unwrap().lookup_inferred(&LocalParamID::from("X")).unwrap();
    assert_eq!(x.normalize(&x_scope), expr("{points: String, name: String}"));
}

/// The output of a write can be written again.
#[test]
fn test_write_field_twice() {
    let engine = graph(
        vec![
            sig_u("() -> ({points: Integer, name: String}, 'points', String, 'name', Boolean)"),
            sig_u(WRITE_FIELD),
            sig_u(WRITE_FIELD),
            sig_u("<X extends {}>(X) -> ()"),
        ],
        vec![(0, 1, 0, 0), (0, 1, 1, 1), (0, 1, 2, 2), (1, 2, 0, 0), (0, 2, 3, 1), (0, 2, 4, 2), (2, 3, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    assert_eq!(engine.validate(&scopes), []);
    let (x, x_scope) = scopes.get(&NodeIndex::from(3)).unwrap().lookup_inferred(&LocalParamID::from("X")).unwrap();
    assert_eq!(x.normalize(&x_scope), expr("{points: String, name: Boolean}"));
}

/// The source of a mapped input whose values don't depend on the source is inferred from the keys of what flows in.
#[test]
fn test_infer_source_from_keys() {
    let engine = graph(
        vec![sig_u("() -> ({a: Integer, b: Float})"), sig_u("<C extends {}>({[K in keyof C]: Comparable}) -> (C)")],
        vec![(0, 1, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    assert_eq!(engine.validate(&scopes), []);
    let (c, c_scope) = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("C")).unwrap();
    assert_eq!(c.normalize(&c_scope), expr("{a: Integer, b: Float}"));
}

#[test]
fn test_mapped_input_rejects_a_field() {
    let engine = graph(
        vec![sig_u("() -> ({a: Integer, b: String})"), sig_u("<C extends {}>({[K in keyof C]: Comparable}) -> (C)")],
        vec![(0, 1, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    assert_ne!(engine.validate(&scopes), []);
}

fn common_scope(params: &str) -> nodety::scope::Scope<nodety::demo_type::DemoType> {
    params.parse().unwrap()
}

const OVERRIDE_B: &str = "<T extends {b: Integer}>(T) -> ({[K in keyof T]: K extends 'b' ? Float : (T[K])})";

/// A field the mapped type passes through keeps its union type, so it fits a union bound. Fields used to stay lazy
/// conditionals, and subtyping split the union parent before evaluating the child.
#[test]
fn test_passed_through_union_field_fits_a_union() {
    let engine = graph(
        vec![
            sig_u("() -> ({a: String | Unit, b: Integer})"),
            sig_u(OVERRIDE_B),
            sig_u("({a: String | Unit}) -> ()"),
            sig_u("<X extends {a: String | Unit}>(X) -> ()"),
        ],
        vec![(0, 1, 0, 0), (1, 2, 0, 0), (1, 3, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    assert_eq!(engine.validate(&scopes), []);
    let (x, x_scope) = scopes.get(&NodeIndex::from(3)).unwrap().lookup_inferred(&LocalParamID::from("X")).unwrap();
    assert_eq!(x.normalize(&x_scope), expr("{a: String | Unit, b: Float}"));
}
