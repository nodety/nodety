use crate::common::{expr, expr_u, sig, sig_u};
use assert_matches::assert_matches;
use nodety::{
    Edge, Node, Nodety,
    demo_type::DemoType,
    inference::{InferenceConfig, Scopes},
    scope::ScopePointer,
    type_expr::{ScopedTypeExpr, TypeExpr},
    validation::{GraphLocation, ValidationError, ValidationErrorKind},
};
use petgraph::graph::NodeIndex;

mod common;

fn inferred(scopes: &Scopes<DemoType>, node_idx: NodeIndex, param: &str) -> ScopedTypeExpr<DemoType> {
    let (inferred, scope) = scopes[&node_idx].lookup_inferred(&param.into()).expect("param is inferred");
    inferred.normalize(&scope)
}

#[test]
fn notation_round_trips() {
    for notation in [
        "() -> () ! Nondeterministic",
        "() -> () ! Nondeterministic | Fallible",
        "<E, T, U>(Array<T>, (T) -> (U) ! E) -> (Array<U>) ! E",
        "<E>() -> () ! E handles Fallible",
        "() -> () handles Fallible | Nondeterministic",
        "(((T) -> (U) ! E) | Integer) -> ()",
        "(((T) -> (U) handles E) | Integer) -> ()",
        "() -> () ! ((A) -> (B) ! C)",
        "() -> () ! (A & B)",
    ] {
        assert_eq!(notation, sig_u(notation).to_string());
    }
}

#[test]
fn effects_default_to_never() {
    let signature = sig_u("(Integer) -> (Integer)");
    assert_eq!(signature.effects, TypeExpr::Never);
    assert_eq!(signature.handles, TypeExpr::Never);
}

#[test]
fn effects_bind_to_the_signature() {
    assert_eq!(sig_u("() -> () ! A | B").effects, expr_u("A | B"));
    assert_eq!(
        expr_u("((T) -> (U) ! E) | Integer"),
        TypeExpr::Union(Box::new(sig_u("(T) -> (U) ! E").into()), Box::new(expr_u("Integer")))
    );
}

#[test]
fn fewer_effects_are_a_subtype() {
    let pure = sig("(Integer) -> (Integer)");
    let nondeterministic = sig("(Integer) -> (Integer) ! Nondeterministic");
    let both = sig("(Integer) -> (Integer) ! Nondeterministic | Fallible");

    assert!(nondeterministic.clone().supertype_of(pure.clone()).is_supertype());
    assert!(!pure.clone().supertype_of(nondeterministic.clone()).is_supertype());
    assert!(both.clone().supertype_of(nondeterministic.clone()).is_supertype());
    assert!(!nondeterministic.supertype_of(both.clone()).is_supertype());
    assert!(sig("(Integer) -> (Integer) ! Any").supertype_of(both).is_supertype());
}

#[test]
fn effects_of_a_closure_input_get_inferred() {
    let mut nodety = Nodety::<DemoType>::new();
    let mapper = nodety.add_node(sig_u("() -> ((Integer) -> (String) ! Nondeterministic)")).unwrap();
    let map = nodety.add_node(sig_u("<T, U, E>((T) -> (U) ! E) -> () ! E")).unwrap();
    nodety.add_edge(mapper, map, Edge { source_port: 0, target_port: 0 }).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, map, "E"), expr("Nondeterministic"));
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn closure_with_effects_is_rejected_by_a_pure_input() {
    let mut nodety = Nodety::<DemoType>::new();
    let mapper = nodety.add_node(sig_u("() -> ((Integer) -> (String) ! Nondeterministic)")).unwrap();
    let map = nodety.add_node(sig_u("<T, U>((T) -> (U)) -> ()")).unwrap();
    nodety.add_edge(mapper, map, Edge { source_port: 0, target_port: 0 }).unwrap();

    let errors = nodety.validate(&nodety.infer(&InferenceConfig::default()));

    assert_matches!(&errors[..], [ValidationError { kind: ValidationErrorKind::TypeMismatch(_), .. }]);
}

#[test]
fn effects_of_children_flow_into_the_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("<E>() -> () ! E")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () ! Nondeterministic"), parent)).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () ! Fallible"), parent)).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> ()"), parent)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    let effects = inferred(&scopes, parent, "E");
    let root = ScopePointer::new_root();
    assert!(effects.supertype_of(&expr("Nondeterministic | Fallible"), &root, &root).is_supertype());
    assert!(expr("Nondeterministic | Fallible").supertype_of(&effects, &root, &root).is_supertype());
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn children_without_effects_infer_none() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("<E>() -> () ! E")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> ()"), parent)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, parent, "E"), expr("Never"));
}

#[test]
fn handled_effects_dont_flow_into_the_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("<E>() -> () ! E handles Fallible")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () ! Nondeterministic | Fallible"), parent)).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () ! Fallible"), parent)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    let effects = inferred(&scopes, parent, "E");
    let root = ScopePointer::new_root();
    assert!(expr("Nondeterministic").supertype_of(&effects, &root, &root).is_supertype(), "{effects:?}");
    assert!(effects.supertype_of(&expr("Nondeterministic"), &root, &root).is_supertype(), "{effects:?}");
    assert_eq!(nodety.validate(&scopes), []);
}

/// The effects of the inner node are only known once they got inferred from its input.
#[test]
fn inferred_effects_of_children_flow_into_the_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("<E>() -> () ! E handles Fallible")).unwrap();
    let mapper = nodety
        .add_node(Node::new_child(sig_u("() -> ((Integer) -> (String) ! Nondeterministic | Fallible)"), parent))
        .unwrap();
    let map = nodety.add_node(Node::new_child(sig_u("<T, U, F>((T) -> (U) ! F) -> () ! F"), parent)).unwrap();
    nodety.add_edge(mapper, map, Edge { source_port: 0, target_port: 0 }).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    let effects = inferred(&scopes, parent, "E");
    let root = ScopePointer::new_root();
    assert!(expr("Nondeterministic").supertype_of(&effects, &root, &root).is_supertype(), "{effects:?}");
    assert!(effects.supertype_of(&expr("Nondeterministic"), &root, &root).is_supertype(), "{effects:?}");
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn effects_must_be_allowed_by_the_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("() -> () ! Nondeterministic")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () ! Nondeterministic"), parent)).unwrap();
    let failing = nodety.add_node(Node::new_child(sig_u("() -> () ! Fallible"), parent)).unwrap();

    let errors = nodety.validate(&nodety.infer(&InferenceConfig::default()));

    assert_matches!(
        &errors[..],
        [ValidationError { location: GraphLocation::Node { node_idx }, kind: ValidationErrorKind::UnhandledEffects(_) }]
            if *node_idx == failing.index()
    );
}

#[test]
fn handled_effects_are_allowed() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("() -> () handles Fallible")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () ! Fallible"), parent)).unwrap();

    assert_eq!(nodety.validate(&nodety.infer(&InferenceConfig::default())), []);
}

#[test]
fn nodes_without_parent_are_bounded_on_request() {
    let mut nodety = Nodety::<DemoType>::new();
    let node = nodety.add_node(sig_u("() -> () ! Fallible")).unwrap();
    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(nodety.validate(&scopes), []);
    assert_eq!(nodety.validate_with_effect_bound(&scopes, &TypeExpr::Type(DemoType::Fallible)), []);
    assert_matches!(
        &nodety.validate_with_effect_bound(&scopes, &TypeExpr::Never)[..],
        [ValidationError { location: GraphLocation::Node { node_idx }, kind: ValidationErrorKind::UnhandledEffects(_) }]
            if *node_idx == node.index()
    );
}

/// Without anything else to infer it from, an effect variable of a child takes what its parent allows.
#[test]
fn effects_of_a_child_get_inferred_from_the_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("() -> () ! Nondeterministic")).unwrap();
    let child = nodety.add_node(Node::new_child(sig_u("<E>() -> () ! E"), parent)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, child, "E"), expr("Nondeterministic"));
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn uninferred_effects_are_reported() {
    let mut nodety = Nodety::<DemoType>::new();
    let node = nodety.add_node(sig_u("<E>() -> () ! E")).unwrap();
    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(nodety.validate(&scopes), []);
    assert_matches!(
        &nodety.validate_with_effect_bound(&scopes, &TypeExpr::Never)[..],
        [ValidationError { location: GraphLocation::Node { node_idx }, kind: ValidationErrorKind::InsufficientlyInferredTypes }]
            if *node_idx == node.index()
    );
}
