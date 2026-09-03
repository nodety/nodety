use crate::{
    scope::ScopePointer,
    r#type::Type,
    type_expr::{AsScopedRef, ConstructorParams, ScopedRefView, ScopedTypeExpr, TypeExpr},
};

impl<T: Type, Ra: AsScopedRef<T>> TypeExpr<T, Ra> {
    /// # Returns
    /// - [TypeExpr::Never] if `a` and `b` have nothing in common.
    /// - `None` if an uninferred variable prevents the intersection from being known.
    pub fn intersection<Rb: AsScopedRef<T>>(
        a: &Self,
        b: &TypeExpr<T, Rb>,
        scope_a: &ScopePointer<T>,
        scope_b: &ScopePointer<T>,
    ) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        match (a, b) {
            (TypeExpr::Any, b) => Some((b.clone().into_scoped(), ScopePointer::clone(scope_b))),
            (a, TypeExpr::Any) => Some((a.clone().into_scoped(), ScopePointer::clone(scope_a))),
            (TypeExpr::Never, _) => Some((TypeExpr::Never, ScopePointer::clone(scope_a))),
            (_, TypeExpr::Never) => Some((TypeExpr::Never, ScopePointer::clone(scope_b))),

            // References. Ordering mirrors the flat match this used to be:
            // type params on either side first, then a's scope portal, then b's.
            (TypeExpr::Ref(ref_a), TypeExpr::Ref(ref_b)) => match (ref_a.view(), ref_b.view()) {
                (ScopedRefView::Param(param_a), ScopedRefView::Param(param_b)) => {
                    let (_var_a, param_scope_a) = scope_a.lookup(&param_a.param_id)?;
                    let (_var_b, param_scope_b) = scope_b.lookup(&param_b.param_id)?;
                    // First check if the two variables reference the same var.
                    if param_a.param_id == param_b.param_id && param_scope_a == param_scope_b {
                        return Some((a.clone().into_scoped(), ScopePointer::clone(scope_a)));
                    }
                    let (Some((inferred_a, scope_a)), Some((inferred_b, scope_b))) =
                        (scope_a.lookup_inferred(&param_a.param_id), scope_b.lookup_inferred(&param_b.param_id))
                    else {
                        return None;
                    };
                    TypeExpr::intersection(&inferred_a, &inferred_b, &scope_a, &scope_b)
                }
                (ScopedRefView::Param(param_a), _) => {
                    let (inferred_a, scope_a) = scope_a.lookup_inferred(&param_a.param_id)?;
                    TypeExpr::intersection(&inferred_a, b, &scope_a, scope_b)
                }
                (_, ScopedRefView::Param(param_b)) => {
                    let (inferred_b, scope_b) = scope_b.lookup_inferred(&param_b.param_id)?;
                    TypeExpr::intersection(a, &inferred_b, scope_a, &scope_b)
                }
                (ScopedRefView::ScopedExpr { expr, scope: portal }, _) => {
                    TypeExpr::intersection(expr, b, portal, scope_b)
                }
            },

            (TypeExpr::Ref(ref_a), b) => match ref_a.view() {
                ScopedRefView::Param(param_a) => {
                    let (inferred_a, scope_a) = scope_a.lookup_inferred(&param_a.param_id)?;
                    TypeExpr::intersection(&inferred_a, b, &scope_a, scope_b)
                }
                ScopedRefView::ScopedExpr { expr, scope: portal } => TypeExpr::intersection(expr, b, portal, scope_b),
            },

            (a, TypeExpr::Ref(ref_b)) => match ref_b.view() {
                ScopedRefView::Param(param_b) => {
                    let (inferred_b, scope_b) = scope_b.lookup_inferred(&param_b.param_id)?;
                    TypeExpr::intersection(a, &inferred_b, scope_a, &scope_b)
                }
                ScopedRefView::ScopedExpr { expr, scope: portal } => TypeExpr::intersection(a, expr, scope_a, portal),
            },

            (TypeExpr::Intersection(a_a, a_b), b) => {
                let (intersection_a, intersection_a_scope) = TypeExpr::intersection(a_a, a_b, scope_a, scope_a)?;
                TypeExpr::intersection(&intersection_a, b, &intersection_a_scope, scope_b)
            }
            (a, TypeExpr::Intersection(b_a, b_b)) => {
                let (intersection_b, intersection_b_scope) = TypeExpr::intersection(b_a, b_b, scope_b, scope_b)?;
                TypeExpr::intersection(a, &intersection_b, scope_a, &intersection_b_scope)
            }
            (TypeExpr::Operation { a, b, operator }, b_expr) => {
                let a_normalized = a.normalize_concrete(scope_a)?;
                let b_normalized = b.normalize_concrete(scope_a)?;
                TypeExpr::intersection(&T::operation(&a_normalized, operator, &b_normalized), b_expr, scope_a, scope_b)
            }
            (a_expr, TypeExpr::Operation { a, b, operator }) => {
                let a_normalized = a.normalize_concrete(scope_b)?;
                let b_normalized = b.normalize_concrete(scope_b)?;
                TypeExpr::intersection(a_expr, &T::operation(&a_normalized, operator, &b_normalized), scope_a, scope_b)
            }

            (TypeExpr::Conditional(conditional), b) => {
                TypeExpr::intersection(&conditional.distribute(scope_a)?, b, scope_a, scope_b)
            }
            (a, TypeExpr::Conditional(conditional)) => {
                TypeExpr::intersection(a, &conditional.distribute(scope_b)?, scope_a, scope_b)
            }

            (TypeExpr::Type(a), TypeExpr::Type(b)) if a == b => {
                Some((TypeExpr::Type(a.clone()), ScopePointer::clone(scope_a)))
            }
            (TypeExpr::Constructor { inner, .. }, TypeExpr::Type(inst)) if inner == inst => {
                Some((a.clone().into_scoped(), ScopePointer::clone(scope_a)))
            }
            (TypeExpr::Type(inst), TypeExpr::Constructor { inner, .. }) if inner == inst => {
                Some((b.clone().into_scoped(), ScopePointer::clone(scope_b)))
            }
            (
                TypeExpr::Constructor { inner: inner_a, parameters: parameters_a },
                TypeExpr::Constructor { inner: inner_b, parameters: parameters_b },
            ) if inner_a == inner_b => {
                let mut intersected_params = ConstructorParams::new();
                for ident in parameters_a.keys().chain(parameters_b.keys()) {
                    if intersected_params.contains_key(ident) {
                        continue;
                    }
                    let (intersected_param, intersected_scope) =
                        match (parameters_a.get(ident), parameters_b.get(ident)) {
                            (Some(pa), Some(pb)) => TypeExpr::intersection(pa, pb, scope_a, scope_b)?,
                            (Some(pa), None) => (pa.clone().into_scoped(), ScopePointer::clone(scope_a)),
                            (None, Some(pb)) => (pb.clone().into_scoped(), ScopePointer::clone(scope_b)),
                            (None, None) => unreachable!(),
                        };

                    intersected_params
                        .insert(ident.clone(), TypeExpr::scope_portal(intersected_param, intersected_scope));
                }
                Some((
                    TypeExpr::Constructor { inner: inner_a.clone(), parameters: intersected_params },
                    ScopePointer::clone(scope_a),
                ))
            }
            (TypeExpr::Constructor { .. }, TypeExpr::Constructor { .. }) => {
                Some((TypeExpr::Never, ScopePointer::clone(scope_a)))
            }

            (TypeExpr::Index { expr, index }, b) => {
                let (index_type, index_scope) = expr.index(index, scope_a, scope_a)?;
                TypeExpr::intersection(&index_type, b, &index_scope, scope_b)
            }
            (a, TypeExpr::Index { expr, index }) => {
                let (index_type, index_scope) = expr.index(index, scope_b, scope_b)?;
                TypeExpr::intersection(a, &index_type, scope_a, &index_scope)
            }

            (TypeExpr::KeyOf(expr), b) => {
                let (key_type, key_scope) = expr.keyof(scope_a)?;
                TypeExpr::intersection(&key_type, b, &key_scope, scope_b)
            }
            (a, TypeExpr::KeyOf(expr)) => {
                let (key_type, key_scope) = expr.keyof(scope_b)?;
                TypeExpr::intersection(a, &key_type, scope_a, &key_scope)
            }

            (TypeExpr::NodeSignature(_), _) | (_, TypeExpr::NodeSignature(_)) => {
                Some((TypeExpr::Never, ScopePointer::new_root()))
            }

            (TypeExpr::PortTypes(_), _) | (_, TypeExpr::PortTypes(_)) => {
                Some((TypeExpr::Never, ScopePointer::new_root()))
            }

            // @Todo: Test this
            // type G = Prettify<({ a: number } | { b: number }) & ({ c: string } | { d: boolean })>;
            (TypeExpr::Union(a, b), c) => {
                let (a_intersection, a_scope) = TypeExpr::intersection(a, c, scope_a, scope_b)?;
                let (b_intersection, b_scope) = TypeExpr::intersection(b, c, scope_a, scope_b)?;

                if a_scope == b_scope {
                    Some((TypeExpr::Union(Box::new(a_intersection), Box::new(b_intersection)), a_scope.clone()))
                } else {
                    Some((
                        TypeExpr::Union(
                            Box::new(TypeExpr::scope_portal(a_intersection, a_scope)),
                            Box::new(TypeExpr::scope_portal(b_intersection, b_scope)),
                        ),
                        ScopePointer::new_root(),
                    ))
                }
            }
            (a, TypeExpr::Union(b, c)) => {
                let (b_intersection, b_scope) = TypeExpr::intersection(a, b, scope_a, scope_b)?;
                let (c_intersection, c_scope) = TypeExpr::intersection(a, c, scope_a, scope_b)?;

                if b_scope == c_scope {
                    Some((TypeExpr::Union(Box::new(b_intersection), Box::new(c_intersection)), b_scope.clone()))
                } else {
                    Some((
                        TypeExpr::Union(
                            Box::new(TypeExpr::scope_portal(b_intersection, b_scope)),
                            Box::new(TypeExpr::scope_portal(c_intersection, c_scope)),
                        ),
                        ScopePointer::new_root(),
                    ))
                }
            }

            (TypeExpr::Type(_), _) => Some((TypeExpr::Never, ScopePointer::clone(scope_a))),
            (_, TypeExpr::Type(_)) => Some((TypeExpr::Never, ScopePointer::clone(scope_a))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::parse::expr;

    #[test]
    fn test_intersection() {
        let scope = ScopePointer::new_root();
        assert_eq!(
            expr("{a: Integer, b: String}"),
            TypeExpr::intersection(&expr("{a: Integer}"), &expr("{b: String}"), &scope, &scope)
                .unwrap()
                .0
                .normalize(&scope),
        );
    }
}
