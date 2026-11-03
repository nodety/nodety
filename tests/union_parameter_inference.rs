//! Inference against a union of a parameter and concrete arms, `P | C`: the arms of the incoming type that `C` already
//! covers are no candidates for `P`.
use crate::common::{expr, graph, sig_u};
use nodety::{inference::InferenceConfig, scope::ScopePointer, type_expr::ScopedTypeExpr};
use petgraph::graph::NodeIndex;
use proptest::prelude::*;

mod common;

const FILTER_MAP: &str = "<I, O>(Array<I>, (I, Integer) -> (O | Unit)) -> (Array<O>)";

/// The `O` that `FILTER_MAP` infers from `closure` on an array of `elements`, and the validation errors.
fn filter_map(elements: &str, closure: &str) -> (ScopedTypeExpr<nodety::demo_type::DemoType>, usize) {
    let source = format!("() -> (Array<{elements}>)");
    let nodety = graph(vec![sig_u(&source), sig_u(closure), sig_u(FILTER_MAP)], vec![(0, 2, 0, 0), (1, 2, 0, 1)]);
    let scopes = nodety.infer(&InferenceConfig::default());
    let errors = nodety.validate(&scopes).len();
    let (o, scope) = scopes[&NodeIndex::new(2)].lookup_inferred(&"O".into()).expect("O is inferred");
    (o.normalize(&scope), errors)
}

fn equivalent(a: &ScopedTypeExpr<nodety::demo_type::DemoType>, b: &str) -> bool {
    let root = ScopePointer::new_root();
    a.supertype_of(&expr(b), &root, &root).is_supertype() && expr(b).supertype_of(a, &root, &root).is_supertype()
}

#[test]
fn a_covered_arm_is_no_candidate() {
    let (o, errors) = filter_map("String", "() -> ((String, Integer) -> ('a' | Unit))");
    assert!(equivalent(&o, "'a'"), "{o:?}");
    assert_eq!(errors, 0);
}

#[test]
fn a_union_kept_type_stays_a_union() {
    let (o, errors) = filter_map("String", "() -> ((String, Integer) -> ('a' | Unit | 'b'))");
    assert!(equivalent(&o, "'a' | 'b'"), "{o:?}");
    assert_eq!(errors, 0);
}

#[test]
fn a_generic_identity_closure_keeps_the_elements() {
    let (o, errors) = filter_map("'a' | Unit", "() -> (<T>(T, Integer) -> (T))");
    assert!(equivalent(&o, "'a'"), "{o:?}");
    assert_eq!(errors, 0);
}

#[test]
fn only_covered_arms_infer_never() {
    let (o, errors) = filter_map("String", "() -> ((String, Integer) -> (Unit))");
    assert!(equivalent(&o, "Never"), "{o:?}");
    assert_eq!(errors, 0);
}

/// `Unit` inside a record field isn't an arm of the incoming union, so it stays.
#[test]
fn unit_inside_a_record_stays() {
    let (o, errors) = filter_map("String", "() -> ((String, Integer) -> ({x: Integer | Unit} | Unit))");
    assert!(equivalent(&o, "{x: Integer | Unit}"), "{o:?}");
    assert_eq!(errors, 0);
}

#[test]
fn arms_the_concrete_arm_does_not_cover_stay() {
    let (o, errors) = filter_map("String", "() -> ((String, Integer) -> ('a' | Integer))");
    assert!(equivalent(&o, "'a' | Integer"), "{o:?}");
    assert_eq!(errors, 0);
}

/// A concrete arm covers every arm it is a supertype of, not only equal ones.
#[test]
fn a_wider_concrete_arm_covers_narrower_arms() {
    let nodety = graph(
        vec![sig_u("() -> ('a' | 'b' | Integer)"), sig_u("<O>(O | String) -> (O)"), sig_u("(Integer) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let scopes = nodety.infer(&InferenceConfig::default());
    let (o, scope) = scopes[&NodeIndex::new(1)].lookup_inferred(&"O".into()).expect("O is inferred");
    assert!(equivalent(&o.normalize(&scope), "Integer"));
    assert_eq!(nodety.validate(&scopes), []);
}

fn kept_arm() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("'a'"), Just("'b'"), Just("Integer"), Just("Boolean"), Just("{x: Integer | Unit}")]
}

proptest! {
    /// Wherever the `Unit` arms are in what the closure returns, `O` is the rest.
    #[test]
    fn o_is_what_the_closure_keeps(
        arms in proptest::collection::vec(kept_arm(), 1..4),
        units in proptest::collection::vec(0usize..5, 1..3),
    ) {
        let mut returned = arms.clone();
        for position in units {
            returned.insert(position.min(returned.len()), "Unit");
        }
        let closure = format!("() -> ((String, Integer) -> ({}))", returned.join(" | "));
        let (o, errors) = filter_map("String", &closure);
        prop_assert!(equivalent(&o, &arms.join(" | ")), "{} gave {o:?}", returned.join(" | "));
        prop_assert_eq!(errors, 0);
    }
}
