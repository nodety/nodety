//! Several record candidates of a type parameter join field by field, see `Type::optional_field_type`.
use crate::common::{expr, graph, sig_u};
use nodety::{inference::InferenceConfig, scope::LocalParamID, scope::ScopePointer};
use petgraph::graph::NodeIndex;

mod common;

/// `sources` flow into `node`, whose output is inferred to the same type as `expected` (each a supertype of the
/// other).
#[track_caller]
fn assert_joins(sources: &[&str], node: &str, expected: &str) {
    let nodes = vec![sig_u(&format!("() -> ({})", sources.join(", "))), sig_u(node), sig_u("<X>(X) -> ()")];
    let mut edges: Vec<_> = (0..sources.len()).map(|port| (0, 1, port, port)).collect();
    edges.push((1, 2, 0, 0));
    let engine = graph(nodes, edges);
    let scopes = engine.infer(&InferenceConfig::default());
    assert_eq!(engine.validate(&scopes), []);
    let (x, scope) = scopes.get(&NodeIndex::from(2)).unwrap().lookup_inferred(&LocalParamID::from("X")).unwrap();
    let (x, expected, root) = (x.normalize(&scope), expr(expected), ScopePointer::new_root());
    let same = x.supertype_of(&expected, &root, &root).is_supertype()
        && expected.supertype_of(&x, &root, &root).is_supertype();
    assert!(same, "inferred {x:?}");
}

#[test]
fn test_shared_fields_unite_and_the_others_become_optional() {
    assert_joins(
        &["{a: Integer, b: String}", "{a: Float, c: Boolean}"],
        "<T>(T, T) -> (T)",
        "{a: Integer | Float, b: String | Unit, c: Boolean | Unit}",
    );
}

/// Before, the candidate with fewer fields was picked: it is a supertype of the other under width subtyping, but the
/// field `b` was lost.
#[test]
fn test_a_record_with_more_fields_keeps_them_optional() {
    assert_joins(&["{a: Integer}", "{a: Integer, b: String}"], "<T>(T, T) -> (T)", "{a: Integer, b: String | Unit}");
}

#[test]
fn test_fields_of_variadic_inputs_join() {
    assert_joins(
        &["{a: Integer}", "{b: String}", "{a: Integer, b: String}"],
        "<T>(...T) -> (T)",
        "{a: Integer | Unit, b: String | Unit}",
    );
}

/// Constructors join parameter by parameter, so records nested in other constructors (tables of rows) join too.
#[test]
fn test_records_in_constructors_join() {
    assert_joins(
        &["Array<{a: Integer, b: String}>", "Array<{a: Integer, c: Float}>"],
        "<T>(...Array<T>) -> (Array<T>)",
        "Array<{a: Integer, b: String | Unit, c: Float | Unit}>",
    );
}

#[test]
fn test_the_join_is_a_supertype_of_every_candidate() {
    let root = ScopePointer::new_root();
    let join = expr("{a: Integer | Float, b: String | Unit, c: Boolean | Unit}");
    for candidate in ["{a: Integer, b: String}", "{a: Float, c: Boolean}", "{a: Integer}"] {
        assert!(join.supertype_of(&expr(candidate), &root, &root).is_supertype(), "{candidate}");
    }
    // `b` is optional, but where a record has it, it must fit.
    assert!(!join.supertype_of(&expr("{a: Integer, b: Float}"), &root, &root).is_supertype());
}

/// A join outside the parameter's bound is not picked: the string literals join to `String`, which the bound doesn't
/// allow, so inference falls back to the union of the candidates.
#[test]
fn test_a_join_outside_the_bound_falls_back() {
    assert_joins(&["{a: 'x'}", "{a: 'y'}"], "<T extends {a: 'x' | 'y'}>(T, T) -> (T)", "{a: 'x'} | {a: 'y'}");
}
