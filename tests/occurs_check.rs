//! A type parameter must never be inferred to a type that reaches the parameter itself, also not through other
//! inferred parameters.
use crate::common::{graph, sig_u};
use nodety::{
    TypeExpr,
    demo_type::DemoType,
    inference::InferenceConfig,
    scope::{GlobalParameterId, LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    type_expr::ScopedTypeExpr,
    validation::ValidationErrorKind,
};
use std::collections::HashSet;

mod common;

/// `T := Array<U>` would make `U := Array<T>` infinite.
#[test]
fn indirect_self_reference_is_not_inferred() {
    let engine =
        graph(vec![sig_u("<T>(T) -> (Array<T>)"), sig_u("<U>(U) -> (Array<U>)")], vec![(0, 1, 0, 0), (1, 0, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());
    let inferred = scopes.values().filter(|scope| scope.all_defined().any(|(id, _)| scope.is_inferred(&id))).count();
    assert!(inferred <= 1, "{scopes:?}");
    assert!(
        engine.validate(&scopes).iter().all(|error| error.kind == ValidationErrorKind::InsufficientlyInferredTypes)
    );
}

/// `P1 = {a: P0, b: P0}`, `P2 = {a: P1, b: P1}`, ... reach `P0` 2^n times, but are walked once each.
#[test]
fn nested_inferred_types_are_walked_once() {
    const DEPTH: u32 = 64;
    let mut scope = Scope::new_root();
    for i in 0..=DEPTH {
        scope.define(LocalParamID(i), TypeParameter::default());
    }
    let scope = ScopePointer::new(scope);
    for i in 1..=DEPTH {
        let previous = || ScopedTypeExpr::<DemoType>::param(LocalParamID(i - 1));
        let record = TypeExpr::Constructor {
            inner: DemoType::Record,
            parameters: [("a".into(), previous()), ("b".into(), previous())].into(),
        };
        scope.infer(&LocalParamID(i), record, ScopePointer::clone(&scope)).unwrap();
    }
    let deepest = ScopedTypeExpr::<DemoType>::param(LocalParamID(DEPTH));
    let root = GlobalParameterId { scope: ScopePointer::clone(&scope), local_id: LocalParamID(0) };

    assert!(deepest.references(&HashSet::from([root]), &scope));
    assert!(deepest.contains_uninferred(&scope));
    assert_eq!(deepest.referenced_params(&scope).len(), DEPTH as usize + 1);
}
