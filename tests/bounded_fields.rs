//! Bounded record fields (`L..U`, `=X`) in type parameter bounds.
//!
//! The rule nodes take `Array<C>` where graphite takes `Table<C>`.
use crate::common::{expr, expr_u, graph, sig_u};
use assert_matches::assert_matches;
use nodety::{
    Nodety,
    demo_type::DemoType,
    inference::{InferenceConfig, Scopes},
    scope::{LocalParamID, ScopePointer},
    type_expr::{
        ScopedTypeExpr, TypeExpr,
        subtyping::{DetailedSupertypeDiagnostics, NoSupertypeLayerReason},
    },
    validation::{GraphLocation, ValidationError, ValidationErrorKind},
};
use petgraph::graph::NodeIndex;

mod common;

const WARNINGS_RULE: &str =
    "<C extends {warnings: Integer..Integer | Unit, false_starts: Integer}>(Array<C>) -> (Array<C>)";
const INVARIANT_RULE: &str = "<C extends {name: =String}>(Array<C>) -> (Array<C>)";

#[test]
fn notation_roundtrip() {
    for notation in ["{warnings: Integer..Integer | Unit}", "{name: =String}", "{a: =Integer | Unit, b: Integer}"] {
        assert_eq!(expr_u(notation).to_string(), notation);
    }
    assert_eq!(
        expr("Integer | Unit..Integer | Unit | String"),
        TypeExpr::bounded(expr("Integer | Unit"), expr("Integer | Unit | String"))
    );
    assert_eq!(expr("=Integer"), TypeExpr::invariant(expr("Integer")));
    assert_eq!(expr_u("Array<(=Integer) | String>").to_string(), "Array<(=Integer) | String>");
}

#[test]
fn bounded_as_parent() {
    let bounded = expr("Integer..Integer | Unit");
    for child in ["Integer", "Integer | Unit"] {
        assert!(bounded.supertype_of_naive(&expr(child)).is_supertype(), "{child}");
    }
    for child in ["Unit", "Integer | Unit | String", "Never", "Any"] {
        assert!(!bounded.supertype_of_naive(&expr(child)).is_supertype(), "{child}");
    }

    let invariant = expr("=String");
    assert!(invariant.supertype_of_naive(&expr("String")).is_supertype());
    assert!(!invariant.supertype_of_naive(&expr("'a'")).is_supertype());
    assert!(!invariant.supertype_of_naive(&expr("String | Unit")).is_supertype());
}

/// Everywhere but as a parent, `L..U` is `U`.
#[test]
fn bounded_as_child_is_upper() {
    let bounded = expr("Integer..Integer | Unit");
    assert!(expr("Integer | Unit").supertype_of_naive(&bounded).is_supertype());
    assert!(!expr("Integer").supertype_of_naive(&bounded).is_supertype());
    assert!(bounded.supertype_of_naive(&bounded).is_supertype());
}

#[test]
fn missing_field_needs_optional_lower_bound() {
    assert!(!expr("{w: Integer..Integer | Unit}").supertype_of_naive(&expr("{}")).is_supertype());
    assert!(expr("{w: =Integer | Unit}").supertype_of_naive(&expr("{}")).is_supertype());
}

#[test]
fn normalization_keeps_bounds() {
    assert_eq!(expr("Never..Integer").normalize_naive(), expr("Integer"));
    assert_eq!(expr("{w: Integer..Integer | Unit} & {w: Integer}").normalize_naive(), expr("{w: =Integer}"));
    assert_eq!(
        expr("{w: Integer..Integer | Unit | String} & {w: Integer | Unit..Any}").normalize_naive(),
        expr("{w: Integer | Unit..Integer | Unit | String}")
    );
}

#[test]
fn exact_type_passes() {
    let (engine, scopes) =
        infer(vec!["() -> (Array<{name: String}>)", INVARIANT_RULE, "(Array<{name: String}>) -> ()"]);
    assert_eq!(engine.validate(&scopes), []);
    assert_eq!(inferred_c(&scopes), expr("{name: String}"));
}

#[test]
fn narrower_fails_invariance() {
    let (engine, scopes) = infer(vec!["() -> (Array<{name: 'a'}>)", INVARIANT_RULE]);
    let diagnostics = type_mismatch_at_edge_0(engine.validate(&scopes));
    assert_has_reason(&diagnostics, NoSupertypeLayerReason::BoundedLower);
}

#[test]
fn wider_fails_invariance() {
    let (engine, scopes) = infer(vec!["() -> (Array<{name: String | Unit}>)", INVARIANT_RULE]);
    let diagnostics = type_mismatch_at_edge_0(engine.validate(&scopes));
    assert_has_reason(&diagnostics, NoSupertypeLayerReason::BoundedUpper);
}

#[test]
fn missing_column_fails() {
    let (engine, scopes) = infer(vec!["() -> (Array<{false_starts: Integer}>)", WARNINGS_RULE]);
    let diagnostics = type_mismatch_at_edge_0(engine.validate(&scopes));
    assert_has_reason(&diagnostics, NoSupertypeLayerReason::ConstructorArity);
}

#[test]
fn extra_columns_pass() {
    let (engine, scopes) = infer(vec!["() -> (Array<{name: String, rank: Integer}>)", INVARIANT_RULE]);
    assert_eq!(engine.validate(&scopes), []);
    assert_eq!(inferred_c(&scopes), expr("{name: String, rank: Integer}"));
}

/// The owner insists that both column types work.
#[test]
fn integer_and_optional_integer_pass() {
    for warnings in ["Integer", "Integer | Unit"] {
        let source = format!("() -> (Array<{{warnings: {warnings}, false_starts: Integer, name: String}}>)");
        let (engine, scopes) = infer(vec![&source, WARNINGS_RULE]);
        assert_eq!(engine.validate(&scopes), [], "{warnings}");
        assert_eq!(
            inferred_c(&scopes),
            expr(&format!("{{warnings: {warnings}, false_starts: Integer, name: String}}")),
            "{warnings}"
        );
    }
}

#[test]
fn marker_absent_downstream() {
    let (engine, scopes) = infer(vec![
        "() -> (Array<{warnings: Integer, false_starts: Integer}>)",
        WARNINGS_RULE,
        "<D>(Array<D>) -> (Array<D>)",
    ]);
    assert_eq!(engine.validate(&scopes), []);
    let downstream = scopes[&NodeIndex::new(2)].lookup_inferred(&LocalParamID::from('D')).unwrap();
    assert!(!contains_bounded(&downstream), "{downstream:?}");
}

/// Two rule nodes with different requirements stacked between a source that has both columns and a sink that needs
/// all of them.
#[test]
fn stacked_rules_combine() {
    let source = "() -> (Array<{false_starts: Integer, dqs: String, name: String}>)";
    let rule_a = "<C extends {false_starts: Integer}>(Array<C>) -> (Array<C>)";
    let rule_b = "<C extends {dqs: =String}>(Array<C>) -> (Array<C>)";
    let sink = "(Array<{false_starts: Integer, dqs: String, name: String}>) -> ()";
    let (engine, scopes) = infer(vec![source, rule_a, rule_b, sink]);
    assert_eq!(engine.validate(&scopes), []);
}

/// Infers like graphite does: parameters without a valid candidate fall back to their bound.
fn infer(signatures: Vec<&str>) -> (Nodety<DemoType>, Scopes<DemoType>) {
    let edges = (1..signatures.len()).map(|target| (target - 1, target, 0, 0)).collect();
    let engine = graph(signatures.into_iter().map(sig_u).collect(), edges);
    let scopes = engine.infer(&InferenceConfig::default());
    for scope in scopes.values() {
        scope.infer_bounds();
    }
    (engine, scopes)
}

fn inferred_c(scopes: &Scopes<DemoType>) -> ScopedTypeExpr<DemoType> {
    let (inferred, scope) = scopes[&NodeIndex::new(1)].lookup_inferred(&LocalParamID::from('C')).unwrap();
    inferred.normalize(&scope)
}

fn contains_bounded((expr, scope): &(ScopedTypeExpr<DemoType>, ScopePointer<DemoType>)) -> bool {
    let mut found = false;
    expr.traverse(scope, &mut |expr, _, _| found |= expr.is_bounded(), false);
    found
}

#[track_caller]
fn type_mismatch_at_edge_0(errors: Vec<ValidationError<DemoType>>) -> DetailedSupertypeDiagnostics<DemoType> {
    let [ValidationError { location: GraphLocation::Edge(0), kind: ValidationErrorKind::TypeMismatch(diagnostics) }] =
        errors.as_slice()
    else {
        panic!("expected one type mismatch at edge 0, got {errors:#?}");
    };
    diagnostics.clone()
}

#[track_caller]
fn assert_has_reason(diagnostics: &DetailedSupertypeDiagnostics<DemoType>, reason: NoSupertypeLayerReason) {
    assert_matches!(
        diagnostics.layers().iter().find(|layer| layer.reason.as_ref() == Some(&reason)),
        Some(_),
        "{diagnostics:#?}"
    );
}
