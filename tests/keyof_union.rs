//! `keyof` of a union has every key of every arm; indexing the union with a key an arm lacks indexes that arm as a
//! missing field.
use crate::common::{expr, graph, sig_u};
use nodety::{inference::InferenceConfig, scope::LocalParamID, scope::ScopePointer};
use petgraph::graph::NodeIndex;

mod common;

/// `a` and `b` are each a supertype of the other.
#[track_caller]
fn assert_same(a: &str, b: &str) {
    let (scope, a_expr, b_expr) = (ScopePointer::new_root(), expr(a), expr(b));
    let a_norm = a_expr.normalize(&scope);
    let b_norm = b_expr.normalize(&scope);
    let same = a_norm.supertype_of(&b_norm, &scope, &scope).is_supertype()
        && b_norm.supertype_of(&a_norm, &scope, &scope).is_supertype();
    assert!(same, "{a_norm:?} vs {b_norm:?}");
}

#[test]
fn test_keyof_a_union_has_every_key() {
    assert_same("keyof ({a: Integer, b: String} | {a: Float, c: Boolean})", "'a' | 'b' | 'c'");
    assert_same("keyof ({a: Integer} | {a: Float})", "'a'");
    assert_same("keyof ({a: Integer} | {b: Float} | {c: String})", "'a' | 'b' | 'c'");
}

/// An arm without keys adds none: `keyof ({a: Integer} | Integer)` is `'a'`.
#[test]
fn test_an_arm_without_keys_adds_none() {
    assert_same("keyof ({a: Integer} | Integer)", "'a'");
}

/// Indexing distributes over the arms, so a key only one arm has indexes the other arm as a missing field: what
/// `Type::index` gives for one (`Any` for the demo type, a type system can return its "absent" type instead).
#[test]
fn test_a_key_one_arm_lacks_indexes_it_as_missing() {
    assert_same("({a: Integer, b: String} | {a: Float, c: Boolean})['a']", "Integer | Float");
    assert_same("({a: Integer, b: String} | {a: Float, c: Boolean})['b']", "String | Any");
}

/// `<T, K extends keyof T>`: a key only one arm of the union has is accepted, a key no arm has is not.
#[test]
fn test_a_key_of_one_arm_fits_keyof_the_union() {
    let source = "() -> ({a: Integer, b: String} | {a: Float, c: Boolean}, 'b', 'd')";
    let node = "<T, K extends keyof T>(T, K) -> (T[K])";
    for (key_port, valid) in [(1, true), (2, false)] {
        let engine = graph(
            vec![sig_u(source), sig_u(node), sig_u("<X>(X) -> ()")],
            vec![(0, 1, 0, 0), (0, 1, key_port, 1), (1, 2, 0, 0)],
        );
        let scopes = engine.infer(&InferenceConfig::default());
        assert_eq!(engine.validate(&scopes).is_empty(), valid, "key port {key_port}");
        if valid {
            let (x, scope) =
                scopes.get(&NodeIndex::from(2)).unwrap().lookup_inferred(&LocalParamID::from("X")).unwrap();
            let (x, expected, root) = (x.normalize(&scope), expr("String | Any"), ScopePointer::new_root());
            assert!(x.supertype_of(&expected, &root, &root).is_supertype(), "{x:?}");
        }
    }
}

/// `keyof` stays contravariant for records: a record with more fields has more keys. For unions it is now covariant
/// in the arms: `keyof A ⊆ keyof (A | B)`.
#[test]
fn test_keyof_of_an_arm_is_a_subset_of_keyof_the_union() {
    let root = ScopePointer::new_root();
    let union = expr("keyof ({a: Integer, b: String} | {a: Float, c: Boolean})").normalize(&root);
    let arm = expr("keyof {a: Integer, b: String}").normalize(&root);
    assert!(union.supertype_of(&arm, &root, &root).is_supertype());
    assert!(!arm.supertype_of(&union, &root, &root).is_supertype());
}
