//! Rigid type parameters: the parameters of a generic node while its body (its children) gets checked.
use crate::common::{expr, sig_u};
use nodety::{
    Edge, Node, Nodety,
    demo_type::DemoType,
    inference::{InferenceConfig, Scopes},
    scope::{Scope, ScopeError, ScopePointer},
    type_expr::{
        ScopedTypeExpr, TypeExpr,
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
    let form = inferred.linear_form(&scope).expect("a linear form");
    assert_eq!(form.terms.values().collect::<Vec<_>>(), [&1]);
    assert_eq!(form.constant, Some(TypeExpr::Type(DemoType::IntegerLiteral(1))));

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
