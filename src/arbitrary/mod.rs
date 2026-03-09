//! This module implements the [proptest::arbitrary::Arbitrary] trait for various types.
//!
//! If you implement arbitrary for your [Type]s and operators(or set them to [NoOperator]) [TypeExpr]s you can use proptest
//! to generate arbitrary type expressions and node signatures with your types.
use crate::{
    NoOperator, NodeSignature, Type, TypeExpr,
    demo_type::{DemoOperator, DemoType, SIUnit},
    scope::{LocalParamID, ScopePointer, type_parameter::TypeParameter},
    type_expr::{
        ScopePortal, Unscoped,
        conditional::Conditional,
        node_signature::{port_types::PortTypes, validate_type_parameters},
    },
};
use proptest::collection::btree_map;
use proptest::prelude::*;
use std::collections::HashSet;

#[derive(Clone)]
pub struct ArbitraryExprParams<T: Type> {
    pub expr_strat: BoxedStrategy<TypeExpr<T>>,
}

impl<T: Type + Arbitrary + 'static> Default for ArbitraryExprParams<T>
where
    T::Operator: MaybeArbitraryOperator<T>,
{
    fn default() -> Self {
        Self { expr_strat: any::<TypeExpr<T>>().boxed() }
    }
}

/// Trait for optionally including the Operation variant in the type expression strategy.
/// NoOperator does not implement Arbitrary, so it uses the impl below that returns None.
/// Operator types that implement Arbitrary use the blanket impl that returns a strategy for operators only.
pub trait MaybeArbitraryOperator<T: Type>: Sized {
    fn operator_strategy() -> Option<BoxedStrategy<T::Operator>>;
}

impl<T: Type> MaybeArbitraryOperator<T> for NoOperator {
    fn operator_strategy() -> Option<BoxedStrategy<T::Operator>> {
        None
    }
}

impl<T, O> MaybeArbitraryOperator<T> for O
where
    T: Type<Operator = O> + Arbitrary + 'static,
    O: Arbitrary + 'static,
{
    fn operator_strategy() -> Option<BoxedStrategy<T::Operator>> {
        Some(any::<O>().boxed())
    }
}

impl proptest::arbitrary::Arbitrary for DemoOperator {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<DemoOperator>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        proptest::prop_oneof![
            proptest::strategy::Just(DemoOperator::Multiplication),
            proptest::strategy::Just(DemoOperator::Division),
        ]
        .boxed()
    }
}

impl<T: Type + Arbitrary + 'static> Arbitrary for TypeExpr<T, Unscoped>
where
    T::Operator: MaybeArbitraryOperator<T>,
{
    type Strategy = BoxedStrategy<TypeExpr<T, Unscoped>>;
    type Parameters = ();

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        let leaf = prop_oneof![
            Just(TypeExpr::Any),
            Just(TypeExpr::Never),
            any::<T>().prop_map(TypeExpr::Type),
            // param ids get randomized later when their scope is known
            any::<bool>().prop_map(|infer| TypeExpr::TypeParameter(LocalParamID(0), infer)),
        ];
        let without_operation = leaf
            .prop_recursive(
                4,  // No more than 4 branch levels deep
                64, // Target around 64 total elements
                16, // Each collection is up to 16 elements long
                |element| {
                    let recursive_cases = prop_oneof![
                        (any::<T>(), "[A-Z]", "[A-Z]", element.clone(), element.clone()).prop_map(
                            |(inner, a_ident, b_ident, a, b)| {
                                TypeExpr::Constructor {
                                    inner,
                                    parameters: [(a_ident.to_string(), a), (b_ident.to_string(), b)].into(),
                                }
                            }
                        ),
                        element.clone().prop_map(|expr| TypeExpr::KeyOf(Box::new(expr))),
                        (element.clone(), element.clone()).prop_map(|(expr, index)| {
                            TypeExpr::Index { expr: Box::new(expr), index: Box::new(index) }
                        }),
                        (element.clone(), element.clone()).prop_map(|(a, b)| TypeExpr::Union(Box::new(a), Box::new(b))),
                        (element.clone(), element.clone())
                            .prop_map(|(a, b)| TypeExpr::Intersection(Box::new(a), Box::new(b))),
                        (element.clone(), element.clone()).prop_map(|(a, b)| TypeExpr::Union(Box::new(a), Box::new(b))),
                        (element.clone(), element.clone(), element.clone(), element.clone()).prop_map(
                            |(t_test, t_test_bound, t_then, t_else)| TypeExpr::Conditional(Box::new(Conditional {
                                t_test,
                                t_test_bound,
                                t_then,
                                t_else,
                                infer: HashSet::new(),
                            })),
                        ),
                        any_with::<NodeSignature<T>>(ArbitraryExprParams { expr_strat: element.clone() })
                            .prop_map(|signature| TypeExpr::NodeSignature(Box::new(signature))),
                    ];

                    match T::Operator::operator_strategy() {
                        Some(operator_strategy) => prop_oneof![
                            10 => recursive_cases,
                            1 => (element.clone(), operator_strategy, element.clone()).prop_map(
                                |(a, operator, b)| TypeExpr::Operation {
                                    a: Box::new(a),
                                    operator,
                                    b: Box::new(b),
                                }
                            ),
                        ]
                        .boxed(),
                        None => recursive_cases.boxed(),
                    }
                },
            )
            .boxed();

        without_operation.prop_map(make_expr_valid).boxed()
    }
}

impl<T: Type + Arbitrary + 'static> Arbitrary for PortTypes<T>
where
    T::Operator: MaybeArbitraryOperator<T>,
{
    type Strategy = BoxedStrategy<PortTypes<T>>;
    type Parameters = ArbitraryExprParams<T>;

    fn arbitrary_with(ArbitraryExprParams { expr_strat }: Self::Parameters) -> Self::Strategy {
        (proptest::collection::vec(expr_strat.clone(), 0..2), proptest::option::of(expr_strat))
            .prop_map(|(ports, varg)| PortTypes { ports, varg })
            .boxed()
    }
}

impl<T: Type + Arbitrary + 'static> Arbitrary for NodeSignature<T>
where
    T::Operator: MaybeArbitraryOperator<T>,
{
    type Strategy = BoxedStrategy<NodeSignature<T>>;
    type Parameters = ArbitraryExprParams<T>;

    fn arbitrary_with(params: Self::Parameters) -> Self::Strategy {
        let ports_strategy = prop_oneof![
            // Most common case
            10 => any_with::<PortTypes<T>>(params.clone()).prop_map(|ports| TypeExpr::PortTypes(Box::new(ports))),
            1 => Just(TypeExpr::Never),
            1 => Just(TypeExpr::Any),
            1 => params.expr_strat.clone(),
        ];

        (
            // parameters
            btree_map((0..10u32).prop_map(LocalParamID), any_with::<TypeParameter<T>>(params.clone()), 0..3),
            ports_strategy.clone(),                                // inputs
            ports_strategy.clone(),                                // outputs
            btree_map(0..5usize, params.expr_strat.clone(), 0..3), // default_input_types
            any::<Option<HashSet<u32>>>(),                         // tags
            any::<HashSet<u32>>(),                                 // required_tags
        )
            .prop_map(|(parameters, inputs, outputs, default_input_types, tags, required_tags)| NodeSignature {
                parameters,
                inputs,
                outputs,
                default_input_types,
                tags,
                required_tags,
            })
            .boxed()
    }
}

impl Arbitrary for LocalParamID {
    type Strategy = BoxedStrategy<LocalParamID>;
    type Parameters = ();
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        any::<u32>().prop_map(LocalParamID).boxed()
    }
}

impl<T: Type + Arbitrary + 'static> Arbitrary for TypeParameter<T>
where
    T::Operator: MaybeArbitraryOperator<T>,
{
    type Strategy = BoxedStrategy<TypeParameter<T>>;
    type Parameters = ArbitraryExprParams<T>;

    fn arbitrary_with(ArbitraryExprParams { expr_strat }: Self::Parameters) -> Self::Strategy {
        (proptest::option::of(expr_strat.clone()), proptest::option::of(expr_strat.clone()))
            .prop_map(|(bound, default)| TypeParameter { bound, default })
            .boxed()
    }
}

impl Arbitrary for DemoType {
    type Parameters = ();
    type Strategy = BoxedStrategy<DemoType>;
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(DemoType::Integer),
            Just(DemoType::Float),
            any::<Option<String>>().prop_map(DemoType::String),
            Just(DemoType::Boolean),
            Just(DemoType::Countable),
            Just(DemoType::Comparable),
            Just(DemoType::Sortable),
            Just(DemoType::Unit),
            Just(DemoType::Record),
            Just(DemoType::Array),
            Just(DemoType::AnySI),
            any::<(SIUnit, f64)>().prop_map(|(unit, scale)| DemoType::SI(unit, scale)),
        ]
        .boxed()
    }
}

impl Arbitrary for SIUnit {
    type Parameters = ();
    type Strategy = BoxedStrategy<SIUnit>;
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (any::<i8>(), any::<i8>(), any::<i8>(), any::<i8>(), any::<i8>(), any::<i8>(), any::<i8>())
            .prop_map(|(s, m, kg, a, k, mol, cd)| SIUnit {
                s: s as i16,
                m: m as i16,
                kg: kg as i16,
                a: a as i16,
                k: k as i16,
                mol: mol as i16,
                cd: cd as i16,
            })
            .boxed()
    }
}

/// Mutates an expression to be valid.
/// Faster alternative to filtering invalid expressions (But still quite inefficient).
fn make_expr_valid<T: Type>(expr: TypeExpr<T>) -> TypeExpr<T> {
    let mut scoped: TypeExpr<T, ScopePortal<T>> = expr.into();
    let scope = ScopePointer::new_root();

    loop {
        // When parameters change get cleared, they can't be referenced anymore so we need to revalidate again.
        let mut params_cleared = false;
        scoped.traverse_mut(
            &scope,
            &mut |expr, scope, _is_tl_union| {
                if let TypeExpr::NodeSignature(sig) = expr
                    && validate_type_parameters(&sig.parameters).is_err()
                {
                    sig.parameters.clear();
                    params_cleared = true;
                }
                if let TypeExpr::TypeParameter(param_id, _infer) = expr {
                    if scope.lookup(param_id).is_some() {
                        // Valid, keep
                        return;
                    }
                    let all: Vec<_> = scope.all_defined().map(|(id, _)| id).collect();
                    if let Some(&picked) = all.get(param_id.0 as usize % all.len().max(1)) {
                        *param_id = picked;
                    } else {
                        // There is no param that it could reference.
                        *expr = TypeExpr::Any;
                    }
                }
            },
            false,
        );
        if !params_cleared {
            break;
        }
    }
    scoped.try_into_unscoped().expect("Unscoped can't have scope portals")
}
