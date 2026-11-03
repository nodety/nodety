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
        "() -> () effects <Nondeterministic>",
        "() -> () effects <Nondeterministic> <Fallible>",
        "<E, T, U>(Array<T>, (T) -> (U) effects <E>) -> (Array<U>) effects <E>",
        "<E>() -> () effects <E> handles <Fallible>",
        "() -> () handles <Fallible> <Nondeterministic>",
        "((T) -> (U) effects <E> | Integer) -> ()",
        "((T) -> (U) handles <E> | Integer) -> ()",
        "() -> () effects <(A) -> (B) effects <C>>",
        "() -> () effects <A & B>",
        "() -> () effects <A> <B | C>",
        "() -> () effects <Array<T>>",
        "() -> () effects <<T>(T) -> (T)>",
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
fn effects_are_a_union() {
    assert_eq!(sig_u("() -> () effects <A> <B>").effects, expr_u("A | B"));
    assert_eq!(sig_u("() -> () effects <A | B>").effects, expr_u("A | B"));
}

#[test]
fn whitespace_around_effects_is_optional() {
    let expected = sig_u("() -> () effects <A> <B> handles <C>");
    for notation in [
        "() -> ()effects<A><B>handles<C>",
        "() -> () effects<A> <B>handles <C>",
        "() -> ()  effects  < A >  < B >  handles  < C >",
        "() -> ()\neffects <A>\n  <B>\nhandles <C>",
    ] {
        assert_eq!(sig_u(notation), expected, "{notation}");
    }
    assert_eq!(sig_u("() -> T effects<A>").effects, expr_u("A"));
}

/// The brackets end the effects, so what follows belongs to the surrounding expression.
#[test]
fn effects_end_with_their_brackets() {
    assert_eq!(
        expr_u("(T) -> (U) effects <E> | Integer"),
        TypeExpr::Union(Box::new(sig_u("(T) -> (U) effects <E>").into()), Box::new(expr_u("Integer")))
    );
    let TypeExpr::PortTypes(inputs) = sig_u("((T) -> (U) effects <A>, B) -> ()").inputs else { panic!("ports") };
    assert_eq!(inputs.ports, [sig_u("(T) -> (U) effects <A>").into(), expr_u("B")]);
}

#[test]
fn effects_need_brackets() {
    for notation in ["() -> () effects A", "() -> () effects", "() -> () handles C", "() -> () effects <>"] {
        assert!(notation.parse::<nodety::NodeSignature<DemoType>>().is_err(), "{notation}");
    }
}

#[test]
fn fewer_effects_are_a_subtype() {
    let pure = sig("(Integer) -> (Integer)");
    let nondeterministic = sig("(Integer) -> (Integer) effects <Nondeterministic>");
    let both = sig("(Integer) -> (Integer) effects <Nondeterministic> <Fallible>");

    assert!(nondeterministic.clone().supertype_of(pure.clone()).is_supertype());
    assert!(!pure.clone().supertype_of(nondeterministic.clone()).is_supertype());
    assert!(both.clone().supertype_of(nondeterministic.clone()).is_supertype());
    assert!(!nondeterministic.supertype_of(both.clone()).is_supertype());
    assert!(sig("(Integer) -> (Integer) effects <Any>").supertype_of(both).is_supertype());
}

#[test]
fn effects_of_a_closure_input_get_inferred() {
    let mut nodety = Nodety::<DemoType>::new();
    let mapper = nodety.add_node(sig_u("() -> ((Integer) -> (String) effects <Nondeterministic>)")).unwrap();
    let map = nodety.add_node(sig_u("<T, U, E>((T) -> (U) effects <E>) -> () effects <E>")).unwrap();
    nodety.add_edge(mapper, map, Edge { source_port: 0, target_port: 0 }).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, map, "E"), expr("Nondeterministic"));
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn closure_with_effects_is_rejected_by_a_pure_input() {
    let mut nodety = Nodety::<DemoType>::new();
    let mapper = nodety.add_node(sig_u("() -> ((Integer) -> (String) effects <Nondeterministic>)")).unwrap();
    let map = nodety.add_node(sig_u("<T, U>((T) -> (U)) -> ()")).unwrap();
    nodety.add_edge(mapper, map, Edge { source_port: 0, target_port: 0 }).unwrap();

    let errors = nodety.validate(&nodety.infer(&InferenceConfig::default()));

    assert_matches!(&errors[..], [ValidationError { kind: ValidationErrorKind::TypeMismatch(_), .. }]);
}

#[test]
fn effects_of_children_flow_into_the_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("<E>() -> () effects <E>")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Nondeterministic>"), parent)).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Fallible>"), parent)).unwrap();
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
    let parent = nodety.add_node(sig_u("<E>() -> () effects <E>")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> ()"), parent)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, parent, "E"), expr("Never"));
}

#[test]
fn handled_effects_dont_flow_into_the_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("<E>() -> () effects <E> handles <Fallible>")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Nondeterministic> <Fallible>"), parent)).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Fallible>"), parent)).unwrap();

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
    let parent = nodety.add_node(sig_u("<E>() -> () effects <E> handles <Fallible>")).unwrap();
    let mapper = nodety
        .add_node(Node::new_child(sig_u("() -> ((Integer) -> (String) effects <Nondeterministic> <Fallible>)"), parent))
        .unwrap();
    let map =
        nodety.add_node(Node::new_child(sig_u("<T, U, F>((T) -> (U) effects <F>) -> () effects <F>"), parent)).unwrap();
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
    let parent = nodety.add_node(sig_u("() -> () effects <Nondeterministic>")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Nondeterministic>"), parent)).unwrap();
    let failing = nodety.add_node(Node::new_child(sig_u("() -> () effects <Fallible>"), parent)).unwrap();

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
    let parent = nodety.add_node(sig_u("() -> () handles <Fallible>")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Fallible>"), parent)).unwrap();

    assert_eq!(nodety.validate(&nodety.infer(&InferenceConfig::default())), []);
}

#[test]
fn nodes_without_parent_are_bounded_on_request() {
    let mut nodety = Nodety::<DemoType>::new();
    let node = nodety.add_node(sig_u("() -> () effects <Fallible>")).unwrap();
    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(nodety.validate(&scopes), []);
    assert_eq!(nodety.validate_with_effect_bound(&scopes, &TypeExpr::Type(DemoType::Fallible)), []);
    assert_matches!(
        &nodety.validate_with_effect_bound(&scopes, &TypeExpr::Never)[..],
        [ValidationError { location: GraphLocation::Node { node_idx }, kind: ValidationErrorKind::UnhandledEffects(_) }]
            if *node_idx == node.index()
    );
}

/// Effects are the least solution: an effect variable without anything to infer it from has no effects, whatever its
/// parent allows.
#[test]
fn effects_without_sources_are_never() {
    let mut nodety = Nodety::<DemoType>::new();
    let parent = nodety.add_node(sig_u("() -> () effects <Nondeterministic>")).unwrap();
    let child = nodety.add_node(Node::new_child(sig_u("<E>() -> () effects <E>"), parent)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, child, "E"), expr("Never"));
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn effects_without_sources_satisfy_any_effect_bound() {
    let mut nodety = Nodety::<DemoType>::new();
    let node = nodety.add_node(sig_u("<E>() -> () effects <E>")).unwrap();
    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, node, "E"), expr("Never"));
    assert_eq!(nodety.validate_with_effect_bound(&scopes, &TypeExpr::Never), []);
}

/// Nodes that handle something, nested in each other without effectful children: their effects and handled
/// parameters don't wait on each other.
#[test]
fn nested_handlers_without_effectful_children_have_no_effects() {
    for signature in
        ["<N, Q extends Fallible>() -> () effects <N> handles <Q>", "<N>() -> () effects <N> handles <Fallible>"]
    {
        let mut nodety = Nodety::<DemoType>::new();
        let root = nodety.add_node(sig_u("() -> ()")).unwrap();
        let outer = nodety.add_node(Node::new_child(sig_u(signature), root)).unwrap();
        let inner = nodety.add_node(Node::new_child(sig_u(signature), outer)).unwrap();

        let scopes = nodety.infer(&InferenceConfig::default());

        assert_eq!(inferred(&scopes, inner, "N"), expr("Never"), "{signature}");
        assert_eq!(inferred(&scopes, outer, "N"), expr("Never"), "{signature}");
        assert_eq!(nodety.validate(&scopes), [], "{signature}");
    }
}

/// A handled effect with a parameter, like a disqualification `{key: 'dns', row: R}` with the rows `R` as payload:
/// `R` gets the rows of the matching arms of the children, the other arms stay effects of the parent.
#[test]
fn handled_effects_infer_their_parameters() {
    let mut nodety = Nodety::<DemoType>::new();
    let group = nodety.add_node(sig_u("<E, R>() -> (Array<R>) effects <E> handles <{key: 'dns', row: R}>")).unwrap();
    let both = "() -> () effects <{key: 'dns', row: {a: Integer}}> <{key: 'dnf', row: {b: String}}>";
    nodety.add_node(Node::new_child(sig_u(both), group)).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <{key: 'dns', row: {c: Boolean}}>"), group)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    let rows = inferred(&scopes, group, "R");
    let root = ScopePointer::new_root();
    assert!(rows.supertype_of(&expr("{a: Integer} | {c: Boolean}"), &root, &root).is_supertype(), "{rows:?}");
    assert!(expr("{a: Integer} | {c: Boolean}").supertype_of(&rows, &root, &root).is_supertype(), "{rows:?}");
    let effects = inferred(&scopes, group, "E");
    assert!(effects.supertype_of(&expr("{key: 'dnf', row: {b: String}}"), &root, &root).is_supertype(), "{effects:?}");
    assert!(expr("{key: 'dnf', row: {b: String}}").supertype_of(&effects, &root, &root).is_supertype(), "{effects:?}");
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn every_handled_arm_infers_its_own_parameters() {
    let mut nodety = Nodety::<DemoType>::new();
    let group = nodety
        .add_node(sig_u(
            "<E, R, S>() -> (Array<R>, Array<S>) effects <E> handles <{key: 'dns', row: R}> <{key: 'dnf', row: S}>",
        ))
        .unwrap();
    let both = "() -> () effects <{key: 'dns', row: {a: Integer}}> <{key: 'dnf', row: {b: String}}>";
    nodety.add_node(Node::new_child(sig_u(both), group)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, group, "R"), expr("{a: Integer}"));
    assert_eq!(inferred(&scopes, group, "S"), expr("{b: String}"));
    assert_eq!(inferred(&scopes, group, "E"), expr("Never"));
    assert_eq!(nodety.validate(&scopes), []);
}

/// A parameter as the whole handled effect handles everything its children have.
#[test]
fn a_handled_parameter_takes_every_effect() {
    let mut nodety = Nodety::<DemoType>::new();
    let group = nodety.add_node(sig_u("<E, H>() -> () effects <E> handles <H>")).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Nondeterministic> <Fallible>"), group)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    let handled = inferred(&scopes, group, "H");
    let root = ScopePointer::new_root();
    assert!(handled.supertype_of(&expr("Nondeterministic | Fallible"), &root, &root).is_supertype(), "{handled:?}");
    assert_eq!(inferred(&scopes, group, "E"), expr("Never"));
    assert_eq!(nodety.validate(&scopes), []);
}

/// A bounded parameter as the whole handled effect handles only the effects within its bound: the others are effects
/// of the parent.
#[test]
fn a_bounded_handled_parameter_takes_the_effects_within_its_bound() {
    let mut nodety = Nodety::<DemoType>::new();
    let group = nodety
        .add_node(sig_u(
            "<N extends Nondeterministic, Q extends Fallible>() -> ((Integer) -> (Integer) effects <N> <Q>) \
             effects <N> handles <Q>",
        ))
        .unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Nondeterministic>"), group)).unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Fallible>"), group)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, group, "N"), expr("Nondeterministic"));
    assert_eq!(inferred(&scopes, group, "Q"), expr("Fallible"));
    assert_eq!(nodety.validate(&scopes), []);
}

/// Without a child effect within its bound, a bounded handled parameter handles nothing.
#[test]
fn a_bounded_handled_parameter_without_matching_effects_handles_nothing() {
    let mut nodety = Nodety::<DemoType>::new();
    let root = nodety.add_node(sig_u("() -> ()")).unwrap();
    let group = nodety
        .add_node(Node::new_child(sig_u("<N, Q extends Fallible>() -> () effects <N> handles <Q>"), root))
        .unwrap();
    nodety.add_node(Node::new_child(sig_u("() -> () effects <Nondeterministic>"), group)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, group, "N"), expr("Nondeterministic"));
    assert_eq!(inferred(&scopes, group, "Q"), expr("Never"));
    let errors = nodety.validate(&scopes);
    assert_matches!(
        &errors[..],
        [ValidationError { location: GraphLocation::Node { node_idx }, kind: ValidationErrorKind::UnhandledEffects(_) }]
            if *node_idx == group.index(),
        "{errors:?}"
    );
}

/// A catch node that handles the effect named by its input: `!I` keeps the children's effects from inferring `I`, and
/// the effects of other ids pass through to the outer catch.
fn nested_catches(connect_inner: bool) -> (Nodety<DemoType>, NodeIndex, NodeIndex) {
    let catch = "<I, E>({id: I}) -> () effects <E> handles <{dq: !I}>";
    let mut nodety = Nodety::<DemoType>::new();
    let root = nodety.add_node(sig_u("() -> ()")).unwrap();
    let source_b = nodety.add_node(Node::new_child(sig_u("() -> ({id: 'b'})"), root)).unwrap();
    let outer = nodety.add_node(Node::new_child(sig_u(catch), root)).unwrap();
    nodety.add_edge(source_b, outer, Edge { source_port: 0, target_port: 0 }).unwrap();
    let source_a = nodety.add_node(Node::new_child(sig_u("() -> ({id: 'a'})"), outer)).unwrap();
    let inner = nodety.add_node(Node::new_child(sig_u(catch), outer)).unwrap();
    if connect_inner {
        nodety.add_edge(source_a, inner, Edge { source_port: 0, target_port: 0 }).unwrap();
    }
    nodety.add_node(Node::new_child(sig_u("() -> () effects <{dq: 'a'}> <{dq: 'b'}>"), inner)).unwrap();
    (nodety, outer, inner)
}

#[test]
fn a_handled_effect_named_by_an_input_passes_the_others_on() {
    let (nodety, outer, inner) = nested_catches(true);

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, inner, "I"), expr("'a'"));
    assert_eq!(inferred(&scopes, inner, "E"), expr("{dq: 'b'}"));
    assert_eq!(inferred(&scopes, outer, "I"), expr("'b'"));
    assert_eq!(inferred(&scopes, outer, "E"), expr("Never"));
    assert_eq!(nodety.validate(&scopes), []);
}

#[test]
fn a_handled_effect_named_by_a_missing_input_is_insufficiently_inferred() {
    let (nodety, _, inner) = nested_catches(false);

    let errors = nodety.validate(&nodety.infer(&InferenceConfig::default()));

    assert!(
        errors.iter().any(|error| matches!(
            error,
            ValidationError { location: GraphLocation::Node { node_idx }, kind: ValidationErrorKind::InsufficientlyInferredTypes }
                if *node_idx == inner.index()
        )),
        "{errors:?}"
    );
}

/// A constructor whose concrete parameters rule the other one out gives none of its parameters a candidate.
#[test]
#[allow(clippy::mutable_key_type)] // GlobalParameterId is keyed by scope identity, see lib.rs.
fn a_mismatching_constructor_gives_no_candidates() {
    let mut scope = nodety::scope::Scope::new_root();
    scope.define("R".into(), Default::default());
    let scope = ScopePointer::new(scope);
    let root = ScopePointer::new_root();
    let pattern = expr("{key: 'dns', row: R}");

    let foreign = pattern.collect_candidates(&expr("{key: 'dnf', row: {b: String}}"), &scope, &root, false, false);
    assert!(foreign.is_empty(), "{foreign:?}");
    let matching = pattern.collect_candidates(&expr("{key: 'dns', row: {a: Integer}}"), &scope, &root, false, false);
    assert_eq!(matching.len(), 1);
}

/// Below a parent that bounds its effects, a handled effect with a parameter that no child matches doesn't hide the
/// effects that escape: they are effects of the group, which validation then reports.
#[test]
fn escaping_effects_of_a_group_with_a_handled_parameter_stay_its_effects() {
    let mut nodety = Nodety::<DemoType>::new();
    let root = nodety.add_node(sig_u("() -> () effects <Nondeterministic>")).unwrap();
    let group = nodety
        .add_node(Node::new_child(
            sig_u("<R extends {} = {}, E = Never>() -> (Array<R>) effects <E> handles <{key: 'other', row: R}>"),
            root,
        ))
        .unwrap();
    let throwing = "() -> () effects <Nondeterministic> <{key: 'dns', row: {a: Integer}}>";
    nodety.add_node(Node::new_child(sig_u(throwing), group)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    let effects = inferred(&scopes, group, "E");
    let root_scope = ScopePointer::new_root();
    let escaping = expr("Nondeterministic | {key: 'dns', row: {a: Integer}}");
    assert!(effects.supertype_of(&escaping, &root_scope, &root_scope).is_supertype(), "{effects:?}");
    assert_matches!(
        &nodety.validate(&scopes)[..],
        [ValidationError { location: GraphLocation::Node { node_idx }, kind: ValidationErrorKind::UnhandledEffects(_) }]
            if *node_idx == group.index()
    );
}

#[test]
fn a_handled_parameter_infers_below_a_bounding_parent() {
    let mut nodety = Nodety::<DemoType>::new();
    let root = nodety.add_node(sig_u("() -> () effects <Nondeterministic>")).unwrap();
    let group = nodety
        .add_node(Node::new_child(
            sig_u("<R extends {} = {}, E = Never>() -> (Array<R>) effects <E> handles <{key: 'dns', row: R}>"),
            root,
        ))
        .unwrap();
    let throwing = "() -> () effects <Nondeterministic> <{key: 'dns', row: {a: Integer}}>";
    nodety.add_node(Node::new_child(sig_u(throwing), group)).unwrap();

    let scopes = nodety.infer(&InferenceConfig::default());

    assert_eq!(inferred(&scopes, group, "R"), expr("{a: Integer}"));
    assert_eq!(inferred(&scopes, group, "E"), expr("Nondeterministic"));
    assert_eq!(nodety.validate(&scopes), []);
}
