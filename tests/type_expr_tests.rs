use crate::common::{expr, expr_u, sig};
use maplit::hashset;
use nodety::{
    demo_type::DemoType,
    scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    type_expr::{TypeExpr, subtyping::SupertypeResult},
};
use std::str::FromStr;

mod common;

#[test]
pub fn test_never_of_signatures() {
    let never_of_signatures = sig("Any -> Never");
    assert!(sig("(Integer) -> (String)").supertype_of(never_of_signatures.clone()).is_supertype());

    assert!(sig("<T>(T, String) -> (Integer, T)").supertype_of(never_of_signatures.clone()).is_supertype());
    assert!(!never_of_signatures.clone().supertype_of(sig("<T>(T, String) -> (Integer, T)")).is_supertype());

    assert!(never_of_signatures.clone().supertype_of(never_of_signatures.clone()).is_supertype());
}

#[test]
pub fn test_mother_of_signatures() {
    let mut mother_of_signatures = sig("Never -> Any");
    mother_of_signatures.tags = None;

    let mut child = sig("<T>(T, String) -> (Integer, T)");
    child.tags = Some(hashset! {123});

    assert!(mother_of_signatures.clone().supertype_of(child).is_supertype());

    assert!(mother_of_signatures.clone().supertype_of(mother_of_signatures).is_supertype());
}

#[test]
pub fn test_conditional_type() {
    let non_unit = expr("(String | Unit) extends Unit ? Never : String");

    let scope = ScopePointer::<DemoType>::new(Scope::new_root());

    assert_eq!(expr("String"), non_unit.normalize(&scope));
}

#[test]
pub fn test_conditional_type_2() {
    let non_unit = expr("'abc' extends String ? Integer : Unit");
    assert_eq!(expr("Integer"), non_unit.normalize_naive());
}

#[test]
pub fn test_conditional_type_generic() {
    let non_unit = expr("#0 extends Unit ? Never : #0");

    let mut scope = Scope::new_root();

    scope.define(LocalParamID(0), TypeParameter::default());

    let scope = ScopePointer::new(scope);

    scope.infer(&LocalParamID(0), expr("String|Unit"), ScopePointer::new_root()).unwrap();

    assert_eq!(expr("String"), non_unit.normalize(&scope));

    assert!(non_unit.supertype_of(&expr("String"), &scope, &scope).is_supertype());
    assert!(expr("String").supertype_of(&non_unit, &scope, &scope).is_supertype());
}

#[test]
fn test_generic_index_supertype() {
    let index_expr = expr("#0[Integer]");
    let mut scope = Scope::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    scope.infer(&LocalParamID(0), expr("Array<Integer>"), ScopePointer::new_root()).unwrap();
    let scope = ScopePointer::new(scope);

    assert_eq!(index_expr.supertype_of(&index_expr, &scope, &scope), SupertypeResult::Supertype);
}

#[test]
fn test_generic_keyof_supertype() {
    let keyof_expr = expr("keyof #0");
    let mut scope = Scope::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    scope.infer(&LocalParamID(0), expr("{a: Integer, b: String}"), ScopePointer::new_root()).unwrap();
    let scope = ScopePointer::new(scope);

    let normalized = keyof_expr.normalize(&scope);

    // Order of a and b is not deterministic because of hashmap.
    assert!(expr("'a'|'b'") == normalized || expr("'b'|'a'") == normalized);
}

#[test]
fn test_keyof_any() {
    let scope = ScopePointer::new_root();
    // Used to fail
    assert!(expr("keyof Any").supertype_of(&expr("keyof Any"), &scope, &scope).is_supertype());
}

#[test]
fn test_never_intersection_supertype() {
    let scope = ScopePointer::new_root();
    // Used to fail
    assert!(expr("Integer & Sortable").supertype_of(&expr("Integer & Sortable"), &scope, &scope).is_supertype());
}

#[test]
fn test_keyof_never_intersection() {
    let scope = ScopePointer::new_root();
    // The intersection of a record with Never is Never, so keyof should be Never too
    let normalized = expr("keyof ({a: Integer} & Never)").normalize(&scope);
    assert_eq!(expr("Never"), normalized);
}

#[test]
fn test_keyof_any_intersection() {
    let scope = ScopePointer::new_root();
    // Any & T = T, so keyof(Any & {a: Integer}) = keyof({a: Integer}) = 'a'
    let normalized = expr("keyof (Any & {a: Integer})").normalize(&scope);
    assert_eq!(normalized, expr("'a'"));
}

#[test]
fn test_keyof_union_with_never() {
    let scope = ScopePointer::new_root();
    // Never | T = T, so keyof(Never | {a: Integer}) = keyof({a: Integer}) = 'a'
    let normalized = expr("keyof (Never | {a: Integer})").normalize(&scope);
    assert_eq!(normalized, expr("'a'"));

    // Same when Never is on the right: keyof({a: Integer} | Never) = 'a'
    let normalized = expr("keyof ({a: Integer} | Never)").normalize(&scope);
    assert_eq!(normalized, expr("'a'"));
}

#[test]
fn test_union_supertypes() {
    assert!(expr("keyof {time: Duration | Unit, DNS: Boolean, DNF: Boolean, DQT: Boolean, false-starts: Integer, DQS: Boolean}").supertype_of_naive(&expr("'false-starts' | 'DQT'")).is_supertype());
}

#[test]
fn test_something() {
    let mut scope = Scope::new_root();

    scope.define(LocalParamID(0), TypeParameter::default());
    scope.define(LocalParamID(1), TypeParameter::default());

    scope.infer(&LocalParamID(0), expr("{a: Integer}"), ScopePointer::new_root()).unwrap();
    scope.infer(&LocalParamID(1), expr("{b: Float}"), ScopePointer::new_root()).unwrap();

    let scope = ScopePointer::new(scope);

    assert!(expr("{a: Integer, b: Float}").supertype_of(&expr("#0 & #1"), &scope, &scope).is_supertype());
    assert!(!expr("{a: Integer, c: Float}").supertype_of(&expr("#0 & #1"), &scope, &scope).is_supertype());
}

#[test]
fn test_infer_from() {
    let scope = ScopePointer::new_root();

    let source = expr("<#0>(#0) -> (#0)");
    let target = expr("<#1>(#1) -> (#1)");

    let (_, inferred_scope) = target.build_inferred_child_scope(&source, &scope, &scope);

    let (inferred, _inferred_scope) = inferred_scope.lookup_inferred(&LocalParamID(1)).unwrap();

    assert_eq!(TypeExpr::TypeParameter(LocalParamID(0), true), inferred);
}

#[test]
fn test_operation_supertypes() {
    assert!(expr("Any * Any").supertype_of_naive(&expr("Any * Any")).is_supertype());
}

#[test]
fn test_never_bounded_param_supertype_of_self() {
    let a = expr("<#0 extends Never>(#0) -> ()");
    assert!(a.supertype_of_naive(&a).is_supertype());
}

/// See tsReference.ts IntersectionOfUnions
#[test]
fn test_intersection_of_unions() {
    let a = expr("({ a: Integer } | { b: Integer }) & ({ c: String } | { d: Boolean })");
    let normalized = a.normalize_naive().try_into_unscoped().unwrap();
    let expected = expr_u(
        "({ a: Integer, c: String } | { a: Integer, d: Boolean }) | ({ b: Integer, c: String } | { b: Integer, d: Boolean })",
    );
    assert_eq!(expected, normalized);
}

// ── Intersection tests ──────────────────────────────────────────────────────

#[test]
fn test_intersection_same_type() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Integer"), &expr("Integer"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Integer"));
}

#[test]
fn test_intersection_different_types_is_never() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Integer"), &expr("String"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Never"));
}

#[test]
fn test_intersection_any_left() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Any"), &expr("Integer"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Integer"));
}

#[test]
fn test_intersection_any_right() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Integer"), &expr("Any"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Integer"));
}

#[test]
fn test_intersection_never_left() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Never"), &expr("Integer"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Never"));
}

#[test]
fn test_intersection_never_right() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Integer"), &expr("Never"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Never"));
}

#[test]
fn test_intersection_constructor_and_type_same_inner() {
    let scope = ScopePointer::<DemoType>::new_root();
    let constructor = expr("{a: Integer}");
    let plain_type: TypeExpr<DemoType, _> = TypeExpr::Type(DemoType::Record);
    let (result, _) = TypeExpr::intersection(&constructor, &plain_type, &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("{a: Integer}"));
}

#[test]
fn test_intersection_type_and_constructor_same_inner() {
    let scope = ScopePointer::<DemoType>::new_root();
    let constructor = expr("{a: Integer}");
    let plain_type: TypeExpr<DemoType, _> = TypeExpr::Type(DemoType::Record);
    let (result, _) = TypeExpr::intersection(&plain_type, &constructor, &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("{a: Integer}"));
}

#[test]
fn test_intersection_two_constructors_disjoint_params() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("{a: Integer}"), &expr("{b: String}"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("{a: Integer, b: String}"));
}

#[test]
fn test_intersection_two_constructors_overlapping_params() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) =
        TypeExpr::intersection(&expr("{a: Integer, b: String}"), &expr("{a: Integer, c: Float}"), &scope, &scope)
            .unwrap();
    let normalized = result.normalize(&scope);
    assert!(normalized.supertype_of(&expr("{a: Integer, b: String, c: Float}"), &scope, &scope).is_supertype());
}

#[test]
fn test_intersection_constructors_different_inner_is_never() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Array<Integer>"), &expr("{a: String}"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Never"));
}

#[test]
fn test_intersection_union_distributes_left() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Integer | String"), &expr("Integer"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Integer"));
}

#[test]
fn test_intersection_union_distributes_right() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) = TypeExpr::intersection(&expr("Integer"), &expr("Integer | String"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("Integer"));
}

#[test]
fn test_intersection_node_signatures_is_never() {
    let scope = ScopePointer::<DemoType>::new_root();
    let (result, _) =
        TypeExpr::intersection(&expr("(Integer) -> (String)"), &expr("(String) -> (Integer)"), &scope, &scope).unwrap();
    assert_eq!(result, TypeExpr::Never);
}

#[test]
fn test_intersection_with_generic_params() {
    let mut scope = Scope::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    scope.define(LocalParamID(1), TypeParameter::default());
    let scope = ScopePointer::new(scope);

    scope.infer(&LocalParamID(0), expr("{a: Integer}"), ScopePointer::new_root()).unwrap();
    scope.infer(&LocalParamID(1), expr("{b: String}"), ScopePointer::new_root()).unwrap();

    let (result, _) = TypeExpr::intersection(&expr("#0"), &expr("#1"), &scope, &scope).unwrap();
    assert_eq!(result.normalize(&scope), expr("{a: Integer, b: String}"));
}

// ── Normalization tests ─────────────────────────────────────────────────────

#[test]
fn test_normalize_union_with_any() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Integer | Any").normalize(&scope), TypeExpr::Any);
}

#[test]
fn test_normalize_union_with_never() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Integer | Never").normalize(&scope), expr("Integer"));
}

#[test]
fn test_normalize_union_both_never() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Never | Never").normalize(&scope), expr("Never"));
}

#[test]
fn test_normalize_union_equal_sides() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Integer | Integer").normalize(&scope), expr("Integer"));
}

#[test]
fn test_normalize_union_supertype_subsumes() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Comparable | Integer").normalize(&scope), expr("Comparable"));
}

#[test]
fn test_normalize_intersection_equal_sides() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Integer & Integer").normalize(&scope), expr("Integer"));
}

#[test]
fn test_normalize_intersection_with_any() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Integer & Any").normalize(&scope), expr("Integer"));
}

#[test]
fn test_normalize_intersection_with_never() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Integer & Never").normalize(&scope), expr("Never"));
}

#[test]
fn test_normalize_intersection_supertype_narrowing() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Comparable & Integer").normalize(&scope), expr("Integer"));
}

#[test]
fn test_normalize_constructor_empty_params_to_type() {
    use maplit::btreemap;
    let scope = ScopePointer::<DemoType>::new_root();
    let constructor: TypeExpr<DemoType, _> = TypeExpr::Constructor { inner: DemoType::Array, parameters: btreemap! {} };
    assert_eq!(constructor.normalize(&scope), TypeExpr::Type(DemoType::Array));
}

#[test]
fn test_normalize_type_parameter_uninferred_stays() {
    let mut scope = Scope::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    let scope = ScopePointer::new(scope);
    assert_eq!(expr("#0").normalize(&scope), expr("#0"));
}

#[test]
fn test_normalize_type_parameter_inferred_resolves() {
    let mut scope = Scope::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    scope.infer(&LocalParamID(0), expr("Integer"), ScopePointer::new_root()).unwrap();
    let scope = ScopePointer::new(scope);
    assert_eq!(expr("#0").normalize(&scope), expr("Integer"));
}

#[test]
fn test_normalize_operation() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("SI(1,0,1) / SI(1,1)").normalize(&scope), expr("SI(1,-1,1)"));
}

#[test]
fn test_normalize_index() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("{a: Integer}['a']").normalize(&scope), expr("Integer"));
}

#[test]
fn test_normalize_keyof() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("keyof {a: Integer}").normalize(&scope), expr("'a'"));
}

// ── Subtyping edge case tests ───────────────────────────────────────────────

#[test]
fn test_supertype_any_is_supertype_of_everything() {
    assert!(expr("Any").supertype_of_naive(&expr("Integer")).is_supertype());
    assert!(expr("Any").supertype_of_naive(&expr("String")).is_supertype());
    assert!(expr("Any").supertype_of_naive(&expr("Never")).is_supertype());
    assert!(expr("Any").supertype_of_naive(&expr("{a: Integer}")).is_supertype());
}

#[test]
fn test_supertype_never_is_subtype_of_everything() {
    assert!(expr("Integer").supertype_of_naive(&expr("Never")).is_supertype());
    assert!(expr("String").supertype_of_naive(&expr("Never")).is_supertype());
    assert!(expr("Any").supertype_of_naive(&expr("Never")).is_supertype());
}

#[test]
fn test_supertype_nothing_is_supertype_of_any() {
    assert!(!expr("Integer").supertype_of_naive(&expr("Any")).is_supertype());
    assert!(!expr("String").supertype_of_naive(&expr("Any")).is_supertype());
}

#[test]
fn test_supertype_constructor_with_params() {
    assert!(expr("Array<Integer>").supertype_of_naive(&expr("Array<Integer>")).is_supertype());
    assert!(!expr("Array<Integer>").supertype_of_naive(&expr("Array<String>")).is_supertype());
}

#[test]
fn test_supertype_record_subtyping() {
    assert!(expr("{a: Integer}").supertype_of_naive(&expr("{a: Integer, b: String}")).is_supertype());
    assert!(!expr("{a: Integer, b: String}").supertype_of_naive(&expr("{a: Integer}")).is_supertype());
}

#[test]
fn test_supertype_union_both_sides() {
    assert!(expr("Integer | String").supertype_of_naive(&expr("Integer")).is_supertype());
    assert!(expr("Integer | String").supertype_of_naive(&expr("String")).is_supertype());
    assert!(!expr("Integer | String").supertype_of_naive(&expr("Float")).is_supertype());
}

#[test]
fn test_supertype_child_union() {
    assert!(expr("Comparable").supertype_of_naive(&expr("Integer | Float")).is_supertype());
    assert!(!expr("Integer").supertype_of_naive(&expr("Integer | String")).is_supertype());
}

#[test]
fn test_supertype_constructor_vs_plain_type() {
    assert!(expr("Array").supertype_of_naive(&expr("Array<Integer>")).is_supertype());
    assert!(!expr("Array<Integer>").supertype_of_naive(&expr("Array")).is_supertype());
}

#[test]
fn test_supertype_detailed_diagnostics() {
    let scope = ScopePointer::<DemoType>::new_root();
    let result = expr("Integer").supertype_of_detailed(&expr("String"), &scope, &scope);
    assert!(!result.is_supertype());
}

#[test]
fn test_supertype_node_signature_vs_non_signature() {
    assert!(!expr("(Integer) -> (String)").supertype_of_naive(&expr("Integer")).is_supertype());
    assert!(!expr("Integer").supertype_of_naive(&expr("(Integer) -> (String)")).is_supertype());
}

// ── Conversion tests ────────────────────────────────────────────────────────

#[test]
fn test_try_into_unscoped_success() {
    let scoped = expr("Integer");
    assert_eq!(scoped.try_into_unscoped().unwrap(), expr_u("Integer"));
}

#[test]
fn test_try_into_unscoped_complex() {
    assert!(expr("Integer | String").try_into_unscoped().is_ok());
}

#[test]
fn test_conversion_unscoped_to_scoped() {
    use nodety::type_expr::ScopedTypeExpr;
    let unscoped = expr_u("Integer");
    let scoped: ScopedTypeExpr<DemoType> = unscoped.into();
    assert_eq!(scoped, expr("Integer"));
}

#[test]
fn test_conversion_unscoped_to_erased() {
    use nodety::type_expr::ErasedScopePortal;
    let unscoped = expr_u("Integer | String");
    let erased: TypeExpr<DemoType, ErasedScopePortal> = unscoped.into();
    assert!(matches!(erased, TypeExpr::Union(..)));
}

#[test]
fn test_conversion_scoped_to_erased() {
    use nodety::type_expr::ErasedScopePortal;
    let scoped = expr("Array<Integer>");
    let erased: TypeExpr<DemoType, ErasedScopePortal> = scoped.into();
    assert!(matches!(erased, TypeExpr::Constructor { .. }));
}

#[test]
fn test_node_signature_conversion_unscoped_to_scoped() {
    use nodety::NodeSignature;
    use nodety::type_expr::ScopePortal;
    let sig_u: NodeSignature<DemoType, nodety::type_expr::Unscoped> = NodeSignature::from_str("<T>(T) -> (T)").unwrap();
    let sig_scoped: NodeSignature<DemoType, ScopePortal<DemoType>> = sig_u.into();
    assert!(!sig_scoped.parameters.is_empty());
}

#[test]
fn test_node_signature_conversion_unscoped_to_erased() {
    use nodety::NodeSignature;
    use nodety::type_expr::ErasedScopePortal;
    let sig_u: NodeSignature<DemoType, nodety::type_expr::Unscoped> =
        NodeSignature::from_str("(Integer) -> (String)").unwrap();
    let sig_erased: NodeSignature<DemoType, ErasedScopePortal> = sig_u.into();
    assert!(matches!(sig_erased.inputs, TypeExpr::PortTypes(..)));
}

// ── TypeExpr utility method tests ───────────────────────────────────────────

#[test]
fn test_union_with() {
    assert!(matches!(expr("Integer").union_with(expr("String")), TypeExpr::Union(..)));
}

#[test]
fn test_intersection_with() {
    assert!(matches!(expr("{a: Integer}").intersection_with(expr("{b: String}")), TypeExpr::Intersection(..)));
}

#[test]
fn test_from_unions() {
    let union = TypeExpr::from_unions(expr("Integer"), vec![expr("String"), expr("Float")]);
    let scope = ScopePointer::new_root();
    assert!(union.supertype_of(&expr("Integer"), &scope, &scope).is_supertype());
    assert!(union.supertype_of(&expr("String"), &scope, &scope).is_supertype());
    assert!(union.supertype_of(&expr("Float"), &scope, &scope).is_supertype());
}

#[test]
fn test_contains_type_param() {
    assert!(expr("#0").contains_type_param());
    assert!(expr("Array<#0>").contains_type_param());
    assert!(!expr("Integer").contains_type_param());
    assert!(!expr("Array<Integer>").contains_type_param());
}

#[test]
fn test_references_external_type_param() {
    assert!(expr("#0").references_external_type_param());
    assert!(!expr("Integer").references_external_type_param());
    assert!(!expr("<#0>(#0) -> (#0)").references_external_type_param());
}

#[test]
fn test_contains_specific_type_param() {
    assert!(expr("#0").contains_specific_type_param(&LocalParamID(0)));
    assert!(!expr("#0").contains_specific_type_param(&LocalParamID(1)));
    assert!(expr("#0 | #1").contains_specific_type_param(&LocalParamID(1)));
}

#[test]
fn test_collect_references_type_params() {
    let refs = expr("#0 | #1").collect_references_type_params();
    assert!(refs.contains(&LocalParamID(0)));
    assert!(refs.contains(&LocalParamID(1)));
    assert!(!refs.contains(&LocalParamID(2)));
}

// ── DemoType-specific tests ─────────────────────────────────────────────────

#[test]
fn test_demo_type_supertype_relations() {
    use nodety::Type;
    assert!(DemoType::Comparable.supertype_of(&DemoType::Integer));
    assert!(DemoType::Comparable.supertype_of(&DemoType::Float));
    assert!(!DemoType::Comparable.supertype_of(&DemoType::String(None)));
    assert!(DemoType::Countable.supertype_of(&DemoType::Array));
    assert!(DemoType::Countable.supertype_of(&DemoType::Record));
    assert!(!DemoType::Countable.supertype_of(&DemoType::Integer));
    assert!(DemoType::String(None).supertype_of(&DemoType::String(Some("hello".into()))));
    assert!(!DemoType::String(Some("a".into())).supertype_of(&DemoType::String(Some("b".into()))));
}

#[test]
fn test_demo_type_key_type_record() {
    let scope = ScopePointer::<DemoType>::new_root();
    let result = expr("keyof {a: Integer, b: String}").normalize(&scope);
    assert!(result == expr("'a' | 'b'") || result == expr("'b' | 'a'"));
}

#[test]
fn test_demo_type_key_type_array() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("keyof Array<String>").normalize(&scope), expr("Integer"));
}

#[test]
fn test_demo_type_key_type_empty_record() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("keyof {}").normalize(&scope), expr("Never"));
}

#[test]
fn test_demo_type_index_record() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("{a: Integer, b: String}['a']").normalize(&scope), expr("Integer"));
}

#[test]
fn test_demo_type_index_array() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("Array<String>[Integer]").normalize(&scope), expr("String"));
}

#[test]
fn test_demo_type_si_multiplication() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("SI(1,1) * SI(1,0,1)").normalize(&scope), expr("SI(1,1,1)"));
}

#[test]
fn test_demo_type_si_division() {
    let scope = ScopePointer::<DemoType>::new_root();
    assert_eq!(expr("SI(1,0,1) / SI(1,1)").normalize(&scope), expr("SI(1,-1,1)"));
}

#[test]
fn test_demo_type_si_supertype() {
    use nodety::Type;
    use nodety::demo_type::SIUnit;
    let si_a = DemoType::SI(SIUnit { s: 1, m: 0, kg: 0, a: 0, k: 0, mol: 0, cd: 0 }, 1.0);
    let si_b = DemoType::SI(SIUnit { s: 1, m: 0, kg: 0, a: 0, k: 0, mol: 0, cd: 0 }, 1.0);
    let si_c = DemoType::SI(SIUnit { s: 2, m: 0, kg: 0, a: 0, k: 0, mol: 0, cd: 0 }, 1.0);
    assert!(si_a.supertype_of(&si_b));
    assert!(!si_a.supertype_of(&si_c));
    assert!(DemoType::AnySI.supertype_of(&si_a));
    assert!(DemoType::AnySI.supertype_of(&DemoType::AnySI));
    assert!(!si_a.supertype_of(&DemoType::AnySI));
}
