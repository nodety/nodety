//! Rigid type parameters: the parameters of a generic node while its body (its children) gets checked.
use crate::common::{expr, sig_u};
use nodety::{
    Edge, Node, Nodety,
    demo_type::{DemoOperator, DemoType},
    inference::{InferenceConfig, Scopes},
    scope::{Scope, ScopeError, ScopePointer},
    type_expr::{
        ScopedTypeExpr, ScopedTypeRef, TypeExpr,
        subtyping::{NoSupertypeDiagnostics, SupertypeResult},
    },
    validation::{ValidationError, ValidationErrorKind},
};
use petgraph::graph::NodeIndex;
use std::str::FromStr;

mod common;

/// A program of `(signature, parent)` nodes and `(source, target, source port, target port)` edges.
struct Program {
    nodety: Nodety<DemoType>,
    ids: Vec<NodeIndex>,
    scopes: Scopes<DemoType>,
}

impl Program {
    fn new(nodes: &[(&str, Option<usize>)], edges: &[(usize, usize, usize, usize)]) -> Self {
        let mut nodety = Nodety::new();
        let mut ids: Vec<NodeIndex> = Vec::new();
        for (signature, parent) in nodes {
            let node =
                Node { signature: sig_u(signature), parent: parent.map(|parent| ids[parent]), ..Default::default() };
            ids.push(nodety.add_node(node).unwrap());
        }
        for &(source, target, source_port, target_port) in edges {
            nodety.add_edge(ids[source], ids[target], Edge { source_port, target_port }).unwrap();
        }
        let scopes = nodety.infer(&InferenceConfig::default());
        for scope in scopes.values() {
            scope.infer_bounds();
        }
        Self { nodety, ids, scopes }
    }

    fn errors(&self) -> Vec<ValidationError<DemoType>> {
        self.nodety.validate(&self.scopes)
    }

    fn mismatches(&self) -> usize {
        self.errors().iter().filter(|error| matches!(error.kind, ValidationErrorKind::TypeMismatch(_))).count()
    }

    /// The type `param` of node `node` is inferred to, and the scope to read it in.
    fn inferred(&self, node: usize, param: &str) -> (ScopedTypeExpr<DemoType>, ScopePointer<DemoType>) {
        self.scopes[&self.ids[node]].lookup_inferred(&param.into()).expect("param is inferred")
    }
}

/// The root of a body with the parameter `N` rigid, like a generic node whose body gets checked.
const ROOT: &str = "<rigid N extends Integer>() -> ()";
const ADD: &str = "<A extends Integer, B extends Integer>(A, B) -> (A + B)";

fn rigid_scope(params: &str) -> ScopePointer<DemoType> {
    ScopePointer::new(Scope::from_str(params).unwrap())
}

fn supertype(parent: &str, child: &str, scope: &ScopePointer<DemoType>) -> SupertypeResult<NoSupertypeDiagnostics> {
    expr(parent).supertype_of(&expr(child), scope, scope)
}

#[test]
fn notation_round_trips() {
    for notation in ["<rigid N extends Integer>() -> ()", "<rigid T, U = Integer>(T) -> (U)", "<rigid N = 3>() -> ()"] {
        assert_eq!(sig_u(notation).to_string(), notation);
    }
    assert!(sig_u("<rigid N>() -> ()").parameters[&"N".into()].rigid);
    assert!(!sig_u("<N>() -> ()").parameters[&"N".into()].rigid);
}

#[test]
fn a_parameter_may_be_named_rigid() {
    for notation in ["<rigid>() -> ()", "<rigid extends Integer>() -> ()", "<rigid = Integer>() -> ()"] {
        let signature = sig_u(notation);
        assert!(!signature.parameters[&"rigid".into()].rigid, "{notation}");
    }
    assert!(sig_u("<rigid rigid>() -> ()").parameters[&"rigid".into()].rigid);
}

#[test]
fn a_rigid_parameter_is_never_inferred() {
    let scope = rigid_scope("<rigid N extends Integer>");
    assert_eq!(scope.infer_bounds(), 0);
    assert!(matches!(scope.infer(&"N".into(), expr("3"), ScopePointer::new_root()), Err(ScopeError::ParameterRigid)));
    assert_eq!(scope.uninferred().count(), 0);
    assert!(!expr("N").contains_uninferred(&scope));
}

#[test]
fn as_a_child_a_rigid_parameter_is_its_bound() {
    let scope = rigid_scope("<rigid N extends Integer>");
    assert_eq!(supertype("Integer", "N", &scope), SupertypeResult::Supertype);
    assert_eq!(supertype("Comparable", "N", &scope), SupertypeResult::Supertype);
    assert_eq!(supertype("Integer", "N + 1", &scope), SupertypeResult::Supertype);
    // Final answers, not Unknown: N never gets inferred.
    assert!(matches!(supertype("3", "N", &scope), SupertypeResult::Unrelated(_)));
    assert!(matches!(supertype("String", "N", &scope), SupertypeResult::Unrelated(_)));
    assert!(matches!(supertype("3", "N + 1", &scope), SupertypeResult::Unrelated(_)));
    assert!(matches!(supertype("Never", "N", &scope), SupertypeResult::Unrelated(_)));
}

#[test]
fn as_a_parent_a_rigid_parameter_accepts_only_itself() {
    let scope = rigid_scope("<rigid N extends Integer, rigid M extends Integer, U>");
    assert_eq!(supertype("N", "N", &scope), SupertypeResult::Supertype);
    assert_eq!(supertype("N", "Never", &scope), SupertypeResult::Supertype);
    assert_eq!(supertype("N", "N | Never", &scope), SupertypeResult::Supertype);
    assert_eq!(supertype("N | String", "N", &scope), SupertypeResult::Supertype);
    for child in ["3", "Integer", "Any", "M", "N | 3", "N + 1"] {
        assert!(matches!(supertype("N", child, &scope), SupertypeResult::Unrelated(_)), "{child}");
    }
    // An uninferred child might still become `N`.
    assert_eq!(supertype("N", "U", &scope), SupertypeResult::Unknown);
}

/// The demo type computes `+` and `-` over rigid parameters to a canonical form, which nodety compares structurally.
#[test]
fn operations_over_rigid_parameters_have_a_canonical_form() {
    let scope = rigid_scope("<rigid N extends Integer, rigid M extends Integer>");
    let canonical = |notation: &str| expr(notation).rigid_form(&scope).expect("a rigid form");
    for (notation, expected) in [
        ("(N + 1) + 1", "N + 2"),
        ("1 + N", "N + 1"),
        ("(N + 0) + 1", "N + 1"),
        ("M + N", "N + M"),
        ("(N - M) + N", "(N + N) - M"),
        ("1 - N", "1 - N"),
        ("(N - 3) + 1", "N - 2"),
    ] {
        assert_eq!(canonical(notation), canonical(expected), "{notation}");
    }
    assert_eq!(expr("(N + 1) + 1").normalize(&scope), expr("N + 2").normalize(&scope));
    assert_ne!(canonical("N + 1"), canonical("N + 2"));
    assert_ne!(canonical("N + M"), canonical("N + N"));
}

#[test]
fn operations_over_rigid_parameters_compare_linearly() {
    let scope = rigid_scope("<rigid N extends Integer, rigid M extends Integer>");
    for (parent, child) in [
        ("N + 1", "N + 1"),
        ("N + 1", "1 + N"),
        ("N + 1", "(N + 3) - 2"),
        ("N", "N + 0"),
        ("N", "(N + N) - N"),
        ("N + M", "M + N"),
        ("N - M", "(N + 1) - (M + 1)"),
        ("1 - N", "1 - N"),
        ("N + Integer", "N + 5"),
        ("N - N", "M - M"),
        ("0", "N - N"),
    ] {
        assert_eq!(supertype(parent, child, &scope), SupertypeResult::Supertype, "{parent} ⊇ {child}");
    }
    for (parent, child) in [
        ("N + 1", "N + 2"),
        ("N + 1", "N"),
        ("N + 1", "M + 1"),
        ("N + N", "N"),
        ("N + 1", "4"),
        ("N + 5", "N + Integer"),
    ] {
        assert!(matches!(supertype(parent, child, &scope), SupertypeResult::Unrelated(_)), "{parent} ⊉ {child}");
    }
}

#[test]
fn concrete_operations_still_evaluate() {
    assert_eq!(expr("3 + 1").normalize_naive(), expr("4"));
    assert_eq!(expr("3 - Integer").normalize_naive(), expr("Integer"));
}

#[test]
fn a_rigid_parameter_passes_through_a_body() {
    let program = Program::new(&[(ROOT, None), ("() -> (N)", Some(0)), ("(N) -> ()", Some(0))], &[(1, 2, 0, 0)]);
    assert_eq!(program.errors(), []);
    assert!(program.scopes[&program.ids[0]].lookup_inferred(&"N".into()).is_none());
}

/// Without rigid parameters, `N` would get inferred to 3 and the body would pass for every `N`.
#[test]
fn a_constant_is_no_rigid_parameter() {
    let nodes = [(ROOT, None), ("() -> (3)", Some(0)), ("(N) -> ()", Some(0))];
    assert_eq!(Program::new(&nodes, &[(1, 2, 0, 0)]).mismatches(), 1);

    let flexible = [("<N extends Integer>() -> ()", None), ("() -> (3)", Some(0)), ("(N) -> ()", Some(0))];
    assert_eq!(Program::new(&flexible, &[(1, 2, 0, 0)]).errors(), []);
}

/// `<T>(T) -> (T)` with a body that returns an `Integer`.
#[test]
fn a_generic_body_must_not_return_a_concrete_type() {
    let nodes =
        [("<rigid T>() -> ()", None), ("() -> (T)", Some(0)), ("() -> (Integer)", Some(0)), ("(T) -> ()", Some(0))];
    assert_eq!(Program::new(&nodes, &[(2, 3, 0, 0)]).mismatches(), 1);
    // Through a generic node in between.
    let nodes =
        [("<rigid T>() -> ()", None), ("() -> (Integer)", Some(0)), ("<U>(U) -> (U)", Some(0)), ("(T) -> ()", Some(0))];
    assert_eq!(Program::new(&nodes, &[(1, 2, 0, 0), (2, 3, 0, 0)]).mismatches(), 1);
    let nodes =
        [("<rigid T>() -> ()", None), ("() -> (T)", Some(0)), ("<U>(U) -> (U)", Some(0)), ("(T) -> ()", Some(0))];
    assert_eq!(Program::new(&nodes, &[(1, 2, 0, 0), (2, 3, 0, 0)]).errors(), []);
}

#[test]
fn a_generic_node_infers_a_rigid_parameter() {
    let nodes = [(ROOT, None), ("() -> (N)", Some(0)), ("<U>(U) -> (U)", Some(0)), ("<R>(R) -> ()", Some(0))];
    let program = Program::new(&nodes, &[(1, 2, 0, 0), (2, 3, 0, 0)]);
    assert_eq!(program.errors(), []);
    let (inferred, scope) = program.inferred(3, "R");
    assert_eq!(expr("N").supertype_of(&inferred, &program.scopes[&program.ids[1]], &scope), SupertypeResult::Supertype);
}

#[test]
fn arithmetic_in_a_body() {
    let nodes = |output: &'static str| {
        vec![(ROOT, None), ("() -> (N, 1)", Some(0)), (ADD, Some(0)), (output, Some(0)), ("<R>(R) -> ()", Some(0))]
    };
    let edges = [(1, 2, 0, 0), (1, 2, 1, 1), (2, 3, 0, 0), (2, 4, 0, 0)];
    let program = Program::new(&nodes("(N + 1) -> ()"), &edges);
    assert_eq!(program.errors(), []);
    let (inferred, scope) = program.inferred(4, "R");
    let root = &program.scopes[&program.ids[0]];
    assert_eq!(inferred.rigid_form(&scope), expr("N + 1").rigid_form(root));
    assert_ne!(inferred.rigid_form(&scope), expr("N + 2").rigid_form(root));

    assert_eq!(Program::new(&nodes("(1 + N) -> ()"), &edges).errors(), []);
    assert_eq!(Program::new(&nodes("(Integer) -> ()"), &edges).errors(), []);
    assert_eq!(Program::new(&nodes("(N + 2) -> ()"), &edges).mismatches(), 1);
    assert_eq!(Program::new(&nodes("(N) -> ()"), &edges).mismatches(), 1);
}

/// A node whose own parameter is called `N` as well, inferred to the rigid `N` of the body.
#[test]
fn a_shadowing_parameter_keeps_its_scope() {
    let nodes = |output: &'static str| {
        vec![
            (ROOT, None),
            ("() -> ({len: N})", Some(0)),
            ("<N extends Integer>({len: N}) -> ({len: N + 1})", Some(0)),
            (output, Some(0)),
        ]
    };
    let edges = [(1, 2, 0, 0), (2, 3, 0, 0)];
    assert_eq!(Program::new(&nodes("({len: N + 1}) -> ()"), &edges).errors(), []);
    assert_eq!(Program::new(&nodes("({len: N}) -> ()"), &edges).mismatches(), 1);
    assert_eq!(Program::new(&nodes("({len: Integer}) -> ()"), &edges).errors(), []);
}

#[test]
fn rigid_parameters_in_mapped_types() {
    let nodes = |input: &'static str, output: &'static str| {
        vec![(ROOT, None), (input, Some(0)), ("<S>(S) -> ({[K in keyof S]: S[K]})", Some(0)), (output, Some(0))]
    };
    let edges = [(1, 2, 0, 0), (2, 3, 0, 0)];
    assert_eq!(Program::new(&nodes("() -> ({a: N})", "({a: N}) -> ()"), &edges).errors(), []);
    assert_eq!(Program::new(&nodes("() -> ({a: N})", "({a: 3}) -> ()"), &edges).mismatches(), 1);
    assert_eq!(Program::new(&nodes("() -> ({a: 3})", "({a: N}) -> ()"), &edges).mismatches(), 1);
    // A mapped type over a rigid parameter can't be computed: an error, but no wrong answer.
    let nodes = [
        ("<rigid C extends {a: Integer}>() -> ()", None),
        ("() -> (C)", Some(0)),
        ("<S>(S) -> ({[K in keyof S]: Integer})", Some(0)),
        ("({a: 3}) -> ()", Some(0)),
    ];
    let errors = Program::new(&nodes, &edges).errors();
    assert!(
        errors.iter().all(|error| !matches!(error.kind, ValidationErrorKind::TypeMismatch(_))) && !errors.is_empty(),
        "{errors:?}"
    );
}

type Linear = (TypeExpr<DemoType, ScopedTypeRef<DemoType>>, (i64, i64, i64));

fn operation(a: ScopedTypeExpr<DemoType>, add: bool, b: ScopedTypeExpr<DemoType>) -> ScopedTypeExpr<DemoType> {
    let operator = if add { DemoOperator::Addition } else { DemoOperator::Subtraction };
    TypeExpr::Operation { a: Box::new(a), operator, b: Box::new(b) }
}

/// An expression of `N`, `M` and small literals with `+` and `-`, and its value as `(n, m, constant)`.
fn linear_expression() -> impl proptest::strategy::Strategy<Value = Linear> {
    use proptest::prelude::*;
    let literal = |literal: i64| TypeExpr::Type(DemoType::IntegerLiteral(literal));
    let leaf = prop_oneof![
        Just((TypeExpr::param("N"), (1, 0, 0))),
        Just((TypeExpr::param("M"), (0, 1, 0))),
        (-3i64..4).prop_map(move |value| (literal(value), (0, 0, value))),
    ];
    leaf.prop_recursive(5, 32, 2, |inner| {
        (inner.clone(), any::<bool>(), inner).prop_map(|((a, va), add, (b, vb))| {
            let sign = if add { 1 } else { -1 };
            (operation(a, add, b), (va.0 + sign * vb.0, va.1 + sign * vb.1, va.2 + sign * vb.2))
        })
    })
}

/// `constant ± M ± … ± N ± …`: another shape for the value.
fn reshaped((n, m, constant): (i64, i64, i64)) -> ScopedTypeExpr<DemoType> {
    let terms =
        [("M", m), ("N", n)].into_iter().flat_map(|(param, count)| (0..count.abs()).map(move |_| (param, count > 0)));
    terms.fold(TypeExpr::Type(DemoType::IntegerLiteral(constant)), |expr, (param, add)| {
        operation(expr, add, TypeExpr::param(param))
    })
}

proptest::proptest! {
    /// The canonical form is complete: two expressions have the same form if and only if they have the same value.
    #[test]
    fn equal_values_have_equal_canonical_forms((a, a_value) in linear_expression(), (b, b_value) in linear_expression()) {
        let scope = rigid_scope("<rigid N extends Integer, rigid M extends Integer>");
        let form = |expr: &ScopedTypeExpr<DemoType>| expr.rigid_form(&scope).expect("a rigid form");
        proptest::prop_assert_eq!(form(&a), form(&reshaped(a_value)));
        proptest::prop_assert_eq!(a_value == b_value, form(&a) == form(&b));
        proptest::prop_assert_eq!(a_value == b_value, a.supertype_of(&b, &scope, &scope).is_supertype());
        proptest::prop_assert!(a.supertype_of(&reshaped(a_value), &scope, &scope).is_supertype());
    }
}

/// A parameter in the input of a closure is bounded from above: `(Any) -> ()` takes every `D`, so it doesn't widen `D`.
#[test]
fn a_closure_input_does_not_widen_a_parameter() {
    let generic = "<D>(f: (D) -> () = (Any) -> (), x: D) -> (D)";
    let program = Program::new(
        &[("() -> (Integer)", None), (generic, None), ("<R>(R) -> ()", None)],
        &[(0, 1, 0, 1), (1, 2, 0, 0)],
    );
    assert_eq!(program.errors(), []);
    let (inferred, scope) = program.inferred(2, "R");
    assert_eq!(inferred.normalize(&scope), expr("Integer"));

    // In a body, the same keeps a rigid parameter: before, `D` became `{}` and didn't fit the output `C`.
    let nodes = [
        ("<rigid C extends {}>() -> ()", None),
        ("() -> (C)", Some(0)),
        ("<D extends {}>(f: ({row: D}) -> (Integer) = ({row: {}}) -> (Integer), x: D) -> (D)", Some(0)),
        ("(C) -> ()", Some(0)),
    ];
    assert_eq!(Program::new(&nodes, &[(1, 2, 0, 1), (2, 3, 0, 0)]).errors(), []);

    // The lower bound arrives only once a node in between is inferred: `D` waits for it.
    let nodes = [
        ("<rigid C extends {}>() -> ()", None),
        ("() -> (C)", Some(0)),
        ("<T>(T) -> (Array<T>)", Some(0)),
        ("<D extends {}>(f: ({row: D}) -> (Integer) = ({row: {}}) -> (Integer), xs: Array<D>) -> (D)", Some(0)),
        ("(C) -> ()", Some(0)),
    ];
    assert_eq!(Program::new(&nodes, &[(1, 2, 0, 0), (2, 3, 0, 1), (3, 4, 0, 0)]).errors(), []);
}

/// Behind a portal to its scope, a rigid parameter is still a supertype of itself, also as the bounds of `=N`.
#[test]
fn a_rigid_parameter_behind_a_portal_is_a_supertype_of_itself() {
    let scope = rigid_scope("<rigid N extends Integer>");
    for parent in ["N", "=N"] {
        let parent = TypeExpr::scope_portal(expr(parent), ScopePointer::clone(&scope));
        let result = parent.supertype_of(&expr("N"), &ScopePointer::new_root(), &scope);
        assert_eq!(result, SupertypeResult::Supertype, "{parent:?}");
    }
}

/// A parameter inferred from a rigid parameter of an outer scope stays that parameter in a scope that has another one
/// of the same name: the node's `E` doesn't take the root's place, where its child passes `I` on as `O` and another
/// child takes the root's `E` as `O`.
#[test]
fn an_inferred_parameter_keeps_the_scope_it_was_inferred_in() {
    let program = Program::new(
        &[
            ("<rigid E>() -> ()", None),
            ("() -> (E)", Some(0)),
            ("<I, O, E extends Integer>(I) -> (O)", Some(0)),
            ("() -> (I)", Some(2)),
            ("(O) -> ()", Some(2)),
            ("(E) -> ()", Some(0)),
            ("(O) -> ()", Some(2)),
        ],
        &[(1, 2, 0, 0), (3, 4, 0, 0), (2, 5, 0, 0), (1, 6, 0, 0)],
    );
    assert_eq!(program.errors(), []);
}
