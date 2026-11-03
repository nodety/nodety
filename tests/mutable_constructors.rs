//! Mutable constructors (`MutableArray<T>`, `mut {a: T}`) and the deep read-only view `readonly T`.
//!
//! A type checker for a language with mutable arrays and records needs them: anything that can be written through
//! a reference must keep its exact type, while read-only views may be covariant.
use crate::common::{expr, expr_u, graph, sig_u};
use assert_matches::assert_matches;
use nodety::{
    Nodety,
    demo_type::DemoType,
    inference::{InferenceConfig, Scopes},
    scope::{LocalParamID, Scope, ScopePointer},
    type_expr::{
        ScopedTypeExpr, TypeExpr,
        subtyping::{NoSupertypeLayerReason, SupertypeResult},
    },
    validation::{GraphLocation, ValidationError, ValidationErrorKind},
};
use petgraph::graph::NodeIndex;
use std::str::FromStr;

mod common;

#[test]
fn notation_roundtrip() {
    for notation in [
        "MutableArray<Integer>",
        "mut {a: Integer, b: MutableArray}",
        "readonly T",
        "Array<readonly T>",
        "readonly {[K in keyof T]: T[K]}",
    ] {
        assert_eq!(expr_u(notation).to_string(), notation);
    }
}

/// `=T` can't express mutability: a covariant `Array<Integer>` is accepted where `Array<=Integer>` is expected.
#[test]
fn mutable_array_is_invariant() {
    assert_supertype("MutableArray<Integer>", "MutableArray<Integer>");
    assert_unrelated("MutableArray<Integer | String>", "MutableArray<Integer>");
    assert_unrelated("MutableArray<Integer>", "MutableArray<Integer | String>");
    assert_unrelated("MutableArray<Integer>", "Array<Integer>");
}

#[test]
fn mutable_is_subtype_of_readonly_counterpart() {
    assert_supertype("Array<Integer | String>", "MutableArray<Integer>");
    assert_supertype("{a: Integer | String}", "mut {a: Integer}");
    assert_supertype("Array", "MutableArray<Integer>");
    assert_supertype("Countable", "MutableArray<Integer>");
    assert_unrelated("mut {a: Integer}", "{a: Integer}");
}

/// Writing `b` through the wider view would break other references to the record.
#[test]
fn mutable_record_gains_no_optional_fields() {
    assert_unrelated("mut {a: Integer, b: Integer | Unit}", "mut {a: Integer}");
    assert_supertype("{a: Integer, b: Integer | Unit}", "mut {a: Integer}");
    assert_supertype("mut {a: Integer}", "mut {a: Integer, b: String}");
    assert_unrelated("mut {a: Integer | Unit}", "MutableRecord");
}

/// A bounded parameter accepts every container that can store its lower bound and is read as its upper bound.
#[test]
fn bounded_parameter_of_mutable_constructor() {
    let parent = "MutableArray<Integer..Integer | String>";
    assert_supertype(parent, "MutableArray<Integer>");
    assert_supertype(parent, "MutableArray<Integer | String>");
    assert_unrelated(parent, "MutableArray<String>");

    // Stores `Integer`, reads as `Any`, like the array parameter of `push`.
    assert_supertype("MutableArray<Integer..Any>", "MutableArray<Integer>");
    assert_unrelated("MutableArray<Integer..Any>", "MutableArray<String>");
}

#[test]
fn diagnostics_name_the_mutable_parameter() {
    let scope = ScopePointer::new_root();
    let result =
        expr("MutableArray<Integer | String>").supertype_of_detailed(&expr("MutableArray<Integer>"), &scope, &scope);
    let SupertypeResult::Unrelated(diagnostics) = result else { panic!("{result:?}") };
    assert_matches!(
        diagnostics.layers().last().and_then(|layer| layer.reason.as_ref()),
        Some(NoSupertypeLayerReason::MutableConstructorParam)
    );
}

#[test]
fn intersection_of_mutable_constructors() {
    assert_eq!(expr("MutableArray<Integer> & MutableArray<Integer | String>").normalize_naive(), expr("Never"));
    assert_eq!(expr("MutableArray<Integer> & MutableArray<Integer>").normalize_naive(), expr("MutableArray<Integer>"));
    assert_eq!(expr("mut {a: Integer} & mut {b: String}").normalize_naive(), expr("mut {a: Integer, b: String}"));
}

#[test]
fn readonly_is_deep() {
    assert_eq!(expr("readonly MutableArray<mut {a: MutableArray}>").normalize_naive(), expr("Array<{a: Array}>"));
    assert_eq!(expr("readonly (MutableArray<Integer> | Integer)").normalize_naive(), expr("Array<Integer> | Integer"));
    assert_eq!(expr("readonly Array<Integer>").normalize_naive(), expr("Array<Integer>"));
    // Node signatures can't be written to.
    let signature = "(MutableArray<Integer>) -> ()";
    assert_eq!(expr(&format!("readonly ({signature})")).normalize_naive(), expr(signature));
}

#[test]
fn readonly_subtyping() {
    assert_supertype("readonly MutableArray<Integer>", "MutableArray<Integer>");
    assert_supertype("readonly MutableArray<Integer | String>", "MutableArray<Integer>");
    assert_unrelated("MutableArray<Integer>", "readonly MutableArray<Integer>");
    assert_supertype("Array<Integer>", "readonly MutableArray<Integer>");
}

/// `readonly T` is resolved as soon as `T` is known.
#[test]
fn readonly_of_type_parameter() {
    let scope = ScopePointer::new(Scope::from_str("<T>").unwrap());
    let readonly = expr("readonly {a: T}");
    assert_eq!(readonly.normalize(&scope), expr("{a: readonly T}"));
    assert_eq!(expr("readonly T").supertype_of(&expr("readonly T"), &scope, &scope), SupertypeResult::Supertype);
    assert_eq!(
        expr("readonly T").supertype_of(&expr("MutableArray<Integer>"), &scope, &scope),
        SupertypeResult::Unknown
    );

    scope.infer(&LocalParamID::from('T'), expr("MutableArray<Integer>"), ScopePointer::new_root()).unwrap();
    assert_eq!(readonly.normalize(&scope), expr("{a: Array<Integer>}"));
    assert!(readonly.supertype_of(&expr("{a: MutableArray<Integer>}"), &scope, &scope).is_supertype());
}

/// The parameter of a mutable constructor has to be exactly the element type, even if it is only known from
/// where the container flows to.
#[test]
fn infers_element_type_from_mutable_target() {
    let (engine, scopes) = infer(vec!["() -> ('a')", "<T>(T) -> (MutableArray<T>)", "(MutableArray<String>) -> ()"]);
    assert_eq!(engine.validate(&scopes), []);
    assert_eq!(inferred(&scopes, 1, 'T'), expr("String"));
}

#[test]
fn infers_element_type_from_mutable_source() {
    let engine = graph(
        vec![sig_u("() -> (MutableArray<String>)"), sig_u("() -> ('a')"), sig_u("<T>(MutableArray<T>, T) -> ()")],
        vec![(0, 2, 0, 0), (1, 2, 0, 1)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    assert_eq!(engine.validate(&scopes), []);
    assert_eq!(inferred(&scopes, 2, 'T'), expr("String"));
}

#[test]
fn narrower_element_type_fails() {
    let (engine, scopes) = infer(vec!["() -> (MutableArray<'a'>)", "(MutableArray<String>) -> ()"]);
    let errors = engine.validate(&scopes);
    let [ValidationError { location: GraphLocation::Edge(0), kind: ValidationErrorKind::TypeMismatch(diagnostics) }] =
        errors.as_slice()
    else {
        panic!("expected one type mismatch at edge 0, got {errors:#?}");
    };
    assert_matches!(
        diagnostics.layers().iter().find(|layer| layer.reason == Some(NoSupertypeLayerReason::MutableConstructorParam)),
        Some(_)
    );
}

/// A node can promise not to write to its input by taking it read-only.
#[test]
fn readonly_parameter() {
    let identity = "<T>(readonly T) -> (readonly T)";
    let (engine, scopes) = infer(vec!["() -> (MutableArray<Integer>)", identity, "(Array<Integer>) -> ()"]);
    assert_eq!(engine.validate(&scopes), []);
    assert_eq!(inferred(&scopes, 1, 'T'), expr("MutableArray<Integer>"));

    let (engine, scopes) = infer(vec!["() -> (MutableArray<Integer>)", identity, "(MutableArray<Integer>) -> ()"]);
    assert_matches!(engine.validate(&scopes).as_slice(), [ValidationError { location: GraphLocation::Edge(1), .. }]);
}

#[track_caller]
fn assert_supertype(parent: &str, child: &str) {
    assert_eq!(expr(parent).supertype_of_naive(&expr(child)), SupertypeResult::Supertype, "{parent} :> {child}");
}

#[track_caller]
fn assert_unrelated(parent: &str, child: &str) {
    assert_matches!(
        expr(parent).supertype_of_naive(&expr(child)),
        SupertypeResult::Unrelated(_),
        "{parent} :> {child}"
    );
}

/// A chain of nodes, each connected to the next one.
fn infer(signatures: Vec<&str>) -> (Nodety<DemoType>, Scopes<DemoType>) {
    let edges = (1..signatures.len()).map(|target| (target - 1, target, 0, 0)).collect();
    let engine = graph(signatures.into_iter().map(sig_u).collect(), edges);
    let scopes = engine.infer(&InferenceConfig::default());
    (engine, scopes)
}

fn inferred(scopes: &Scopes<DemoType>, node: usize, param: char) -> ScopedTypeExpr<DemoType> {
    let (inferred, scope) = scopes[&NodeIndex::new(node)].lookup_inferred(&LocalParamID::from(param)).unwrap();
    inferred.normalize(&scope)
}

/// A mapped type keeps the constructor of its source, so it maps a mutable record to a mutable record, and
/// `readonly` evaluates it once its source is known.
#[test]
fn readonly_of_mapped_type() {
    assert_eq!(expr("{[K in keyof mut {a: Integer}]: String}").normalize_naive(), expr("mut {a: String}"));
    assert_eq!(expr("readonly {[K in keyof mut {a: Integer}]: MutableArray}").normalize_naive(), expr("{a: Array}"));

    let scope = ScopePointer::new(Scope::from_str("<T>").unwrap());
    let readonly = expr("readonly {[K in keyof T]: T[K]}");
    assert_matches!(readonly.normalize(&scope), TypeExpr::Readonly(_));
    scope.infer(&LocalParamID::from('T'), expr("mut {a: MutableArray<Integer>}"), ScopePointer::new_root()).unwrap();
    assert_eq!(readonly.normalize(&scope), expr("{a: Array<Integer>}"));
}

/// Like any union parameter, `O | Unit` takes no candidate from the arms that `Unit` already covers, also inside a
/// mutable constructor.
#[test]
fn invariant_union_parameter_skips_covered_arms() {
    let (engine, scopes) = infer(vec!["<O>() -> (MutableArray<O | Unit>)", "(MutableArray<'a' | Unit>) -> ()"]);
    assert_eq!(engine.validate(&scopes), []);
    assert_eq!(inferred(&scopes, 0, 'O'), expr("'a'"));

    let (engine, scopes) = infer(vec!["() -> (MutableArray<'a' | Unit>)", "<I>(MutableArray<I | Unit>) -> ()"]);
    assert_eq!(engine.validate(&scopes), []);
    assert_eq!(inferred(&scopes, 1, 'I'), expr("'a'"));
}
