use crate::{
    scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{ScopePortal, ScopedTypeExpr, TypeExpr, TypeExprScope},
};
use std::{borrow::Cow, collections::HashSet};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// Represents the following: `t_test` extends `t_test_bound` ? `t_then` : `t_else`
/// Besides the infer keyword, works almost exactly like conditional types in typescript.
/// Checkout [this doc](https://www.typescriptlang.org/docs/handbook/2/conditional-types.html) for a good guide on the ts conditionals.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        rename_all = "camelCase",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize, S: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, S: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, S: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct Conditional<T: Type, S: TypeExprScope> {
    pub t_test: TypeExpr<T, S>,
    pub t_test_bound: TypeExpr<T, S>,
    pub t_then: TypeExpr<T, S>,
    pub t_else: TypeExpr<T, S>,
    /// @todo
    pub infer: HashSet<LocalParamID>,
}

pub struct ConditionalDistribution<'a, T: Type> {
    pub new_t_test: Cow<'a, ScopedTypeExpr<T>>,
    pub new_t_test_scope: ScopePointer<T>,
    pub new_then_else_scope: ScopePointer<T>,
}

impl<'a, T: Type> ConditionalDistribution<'a, T> {
    pub fn into_conditional(self, conditional: &Conditional<T, ScopePortal<T>>) -> Conditional<T, ScopePortal<T>> {
        Conditional {
            t_test: self.new_t_test.into_owned(),
            t_test_bound: conditional.t_test_bound.clone(),
            t_then: TypeExpr::ScopePortal {
                expr: Box::new(conditional.t_then.clone()),
                scope: ScopePortal { portal: ScopePointer::clone(&self.new_then_else_scope) },
            },
            t_else: TypeExpr::ScopePortal {
                expr: Box::new(conditional.t_else.clone()),
                scope: ScopePortal { portal: self.new_then_else_scope },
            },
            infer: HashSet::new(),
        }
    }
}

impl<T: Type> Conditional<T, ScopePortal<T>> {
    /// If the conditional can get distributed, returns a union for all the distributions.
    /// If it can't get distributed, checks if `t_test_bound` ⊒ `t_test` holds and returns the
    /// appropriate branch (or non if the relation is unknown).
    ///
    /// # Returns
    /// None if the conditional can't get distributed and the subtyping between t_test and t_test_bound is not yet known.
    pub fn distribute(&self, scope: &ScopePointer<T>) -> Option<ScopedTypeExpr<T>> {
        use super::SupertypeResult::*;
        let (first_dist, remaining_dist) = self.build_conditional_distributions(scope);
        if remaining_dist.is_empty() {
            return match self.t_test_bound.supertype_of(&self.t_test, scope, scope) {
                Supertype => Some(self.t_then.clone()),
                Unrelated(_) => Some(self.t_else.clone()),
                Unknown => return None,
            };
        }
        let mut current = TypeExpr::Conditional(Box::new(first_dist.into_conditional(self)));
        for distribution in remaining_dist {
            current = TypeExpr::Union(
                Box::new(current),
                Box::new(TypeExpr::Conditional(Box::new(distribution.into_conditional(self)))),
            );
        }
        Some(current)
    }

    /// Always returns at least one distribution.
    pub fn build_conditional_distributions<'a>(
        &self,
        scope: &ScopePointer<T>,
    ) -> (ConditionalDistribution<'a, T>, Vec<ConditionalDistribution<'a, T>>) {
        let mut distributions = vec![];
        self.t_test.traverse_union(scope, &mut |union_expr, union_expr_scope| {
            let mut inferred_scope = Scope::new_child(scope);
            if let TypeExpr::TypeParameter(param, _infer) = self.t_test {
                inferred_scope.define(param, TypeParameter::default());
                let _ = inferred_scope.infer(&param, union_expr.clone(), ScopePointer::clone(union_expr_scope));
                // println!("inferred {param:?}={union_expr:?}");
                distributions.push(ConditionalDistribution {
                    new_t_test: Cow::Owned(union_expr.clone()),
                    new_t_test_scope: ScopePointer::clone(union_expr_scope),
                    new_then_else_scope: ScopePointer::new(inferred_scope),
                });
            } else {
                distributions.push(ConditionalDistribution {
                    new_t_test: Cow::Owned(union_expr.clone()),
                    new_t_test_scope: ScopePointer::clone(scope),
                    new_then_else_scope: ScopePointer::clone(union_expr_scope),
                });
            };
        });

        (distributions.pop().expect("Unions was expected to be at least one long"), distributions)
    }
}

// fn infer_param<T: Type>(
//     t_test: &ScopedTypeExpr<T>,
//     t_test_bound: &ScopedTypeExpr<T>,
//     scope: &ScopePointer<T>, // The scope for source and target
//     param: GlobalParameter<T>,
// ) -> TypeExpr<T, ScopePortal<T>> {
//     source.collect_candidates(source, &param, Variance::Covariant, scope, scope, false)
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     pub fn test_build_conditional_distributions() {
//         let mut scope = Scope::new_root();
//         scope.define(LocalParamID(0), TypeParameter::default());
//         scope.infer(&LocalParamID(0), expr("String|Unit"), ScopePointer::new_root());

//         let scope = ScopePointer::new(scope);
//         let t_test = Box::new(expr("#0"));
//         let distributions = build_conditional_distributions(&t_test, &true, &scope);

//         assert!(distributions.len() == 2);
//         assert_eq!(distributions[0].new_t_test.as_ref().clone(), expr("String"));
//         assert_eq!(distributions[1].new_t_test.as_ref().clone(), expr("Unit"));

//         // Check that in each of the distributions #0 is inferred as the narrowed type.
//         assert_eq!(
//             expr("String"),
//             distributions[0].new_then_else_scope.lookup(&LocalParamID(0)).unwrap().1.inferred.clone().unwrap().0
//         );
//         assert_eq!(
//             expr("Unit"),
//             distributions[1].new_then_else_scope.lookup(&LocalParamID(0)).unwrap().1.inferred.clone().unwrap().0
//         );
//     }
// }
