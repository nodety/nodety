//! A child that computes to a union (a conditional, an index or an intersection) must be computed before a union
//! parent is split into its arms: `'a' | 'b'` is a supertype of whatever computes to `'a' | 'b'`, but neither arm
//! alone is.
use crate::common::{expr, graph, sig_u};
use nodety::{
    demo_type::DemoType,
    inference::InferenceConfig,
    scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    type_expr::ScopedTypeExpr,
};
use proptest::prelude::*;

mod common;

/// `O extends Unit ? Never : O` with `O = kept`: `kept` without its `Unit` arms.
fn without_unit(kept: &str) -> (ScopedTypeExpr<DemoType>, ScopePointer<DemoType>) {
    let mut scope = Scope::new_root();
    scope.define(LocalParamID::from("O"), TypeParameter::default());
    let scope = ScopePointer::new(scope);
    scope.infer(&"O".into(), expr(kept), ScopePointer::new_root()).unwrap();
    (expr("O extends Unit ? Never : O"), scope)
}

/// Whether `parent` and `child` are supertypes of each other.
fn equivalent(parent: &str, kept: &str) -> bool {
    let (child, scope) = without_unit(kept);
    let root = ScopePointer::new_root();
    expr(parent).supertype_of(&child, &root, &scope).is_supertype()
        && child.supertype_of(&expr(parent), &scope, &root).is_supertype()
}

#[test]
fn a_conditional_keeps_a_union() {
    assert!(equivalent("'a' | 'b'", "'a' | 'b' | Unit"));
    assert!(equivalent("'a' | 'b'", "Unit | 'a' | 'b'"));
}

#[test]
fn never_arms_disappear_from_larger_unions() {
    assert!(equivalent("'a' | 'b' | Integer", "'a' | Unit | 'b' | Unit | Integer"));
    assert!(equivalent("Never", "Unit | Unit"));
}

#[test]
fn the_conditional_distributes_over_every_arm() {
    let kept = "'a' | Integer | {y: String} | Boolean | Unit";
    assert!(equivalent("'a' | Integer | {y: String} | Boolean", kept));
    let (child, scope) = without_unit(kept);
    let root = ScopePointer::new_root();
    for missing_one in ["Integer | {y: String} | Boolean", "'a' | {y: String} | Boolean", "'a' | Integer | Boolean"] {
        assert!(!expr(missing_one).supertype_of(&child, &root, &scope).is_supertype(), "{missing_one}");
    }
}

/// Only arms that are `Unit` are dropped, not the `Unit` inside a record field.
#[test]
fn unit_inside_records_stays() {
    assert!(equivalent("{x: Integer | Unit} | 'a'", "{x: Integer | Unit} | 'a' | Unit"));
    let (child, scope) = without_unit("{x: Integer | Unit} | Unit");
    let root = ScopePointer::new_root();
    assert!(!expr("{x: Integer}").supertype_of(&child, &root, &scope).is_supertype());
}

#[test]
fn an_index_or_intersection_child_is_computed_first() {
    let root = ScopePointer::new_root();
    assert!(expr("'a' | 'b'").supertype_of(&expr("{x: 'a' | 'b'}['x']"), &root, &root).is_supertype());
    assert!(!expr("'a' | 'b'").supertype_of(&expr("{x: 'a' | Integer}['x']"), &root, &root).is_supertype());
    assert!(
        expr("{a: Integer} | {b: Integer}")
            .supertype_of(&expr("({a: Integer} | {b: Integer}) & {}"), &root, &root)
            .is_supertype()
    );
}

/// The parameters of the conditional are uninferred while the graph is built: `O` gets inferred from the closure.
#[test]
fn a_conditional_output_with_parameters_inferred_from_a_closure() {
    let filter_map = "<I, O>(Array<I>, (I, Integer) -> (O)) -> (Array<O extends Unit ? Never : O>)";
    let validate = |closure: &str, sink: &str| {
        let nodety = graph(
            vec![sig_u("() -> (Array<String>)"), sig_u(closure), sig_u(filter_map), sig_u(sink)],
            vec![(0, 2, 0, 0), (1, 2, 0, 1), (2, 3, 0, 0)],
        );
        nodety.validate(&nodety.infer(&InferenceConfig::default()))
    };
    let keeps_union = "() -> ((String, Integer) -> ('a' | 'b' | Unit))";
    assert_eq!(validate(keeps_union, "(Array<'a' | 'b'>) -> ()"), []);
    assert_eq!(validate("() -> (<T>(T, Integer) -> (T))", "(Array<String>) -> ()"), []);
    assert_eq!(validate("() -> ((String, Integer) -> (Unit))", "(Array<Never>) -> ()"), []);
    assert_ne!(validate(keeps_union, "(Array<'a'>) -> ()"), []);
    assert_ne!(validate(keeps_union, "(Array<Integer>) -> ()"), []);
}

fn kept_arm() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("'a'"),
        Just("'b'"),
        Just("Integer"),
        Just("Float"),
        Just("Boolean"),
        Just("{x: Integer | Unit}"),
        Just("Array<'a' | Unit>"),
    ]
}

proptest! {
    /// `T | Unit` without its `Unit` arms is `T`, wherever the `Unit` arms are.
    #[test]
    fn dropping_unit_keeps_the_rest(arms in proptest::collection::vec(kept_arm(), 1..5), units in proptest::collection::vec(0usize..6, 1..3)) {
        let kept = arms.join(" | ");
        let mut with_unit = arms.clone();
        for position in units {
            with_unit.insert(position.min(with_unit.len()), "Unit");
        }
        prop_assert!(equivalent(&kept, &with_unit.join(" | ")), "{kept} vs {}", with_unit.join(" | "));
    }
}
