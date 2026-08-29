use crate::{
    Type, TypeExpr,
    scope::ScopePointer,
    type_expr::{AsScopePortal, ScopePortal, ScopedTypeExpr},
};

impl<T: Type, S: AsScopePortal<T>> TypeExpr<T, S> {
    /// Computes `self[index_type]`
    ///
    /// # Returns
    /// Some((indexed type, scope))
    ///
    /// If index_type is no legal index type for the type, returns Any.
    ///
    /// or None if
    /// - the index type is unknown due to uninferred vars.
    /// - Intersection or Union with distinct scopes.
    pub fn index<S2: AsScopePortal<T>>(
        &self,
        index_type: &TypeExpr<T, S2>,
        own_scope: &ScopePointer<T>,
        index_scope: &ScopePointer<T>,
    ) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        match self {
            Self::Type(inst) => {
                Some((inst.index(None, &index_type.normalize(index_scope)), ScopePointer::clone(own_scope)))
            }
            Self::Constructor { inner, parameters } => {
                let scoped_parameters: std::collections::BTreeMap<String, ScopedTypeExpr<T>> = parameters
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone().map_scope_portals(&mut |s: S| s.as_scope_portal().clone())))
                    .collect();
                Some((
                    inner.index(Some(&scoped_parameters), &index_type.normalize(index_scope)),
                    ScopePointer::clone(own_scope),
                ))
            }

            // see tsReference.ts
            // @todo test this
            // Distributes over the union
            Self::Union(a, b) => {
                let (a_idx, a_scope) = a.index(index_type, own_scope, index_scope)?;
                let (b_idx, b_scope) = b.index(index_type, own_scope, index_scope)?;
                Some((
                    TypeExpr::Union(
                        Box::new(TypeExpr::ScopePortal {
                            expr: Box::new(a_idx),
                            scope: ScopePortal { portal: a_scope },
                        }),
                        Box::new(TypeExpr::ScopePortal {
                            expr: Box::new(b_idx),
                            scope: ScopePortal { portal: b_scope },
                        }),
                    ),
                    ScopePointer::clone(own_scope),
                ))
            }

            // see tsReference.ts
            // @todo test this
            // Distributes over the intersection
            Self::Intersection(a, b) => {
                let (a_idx, a_scope) = a.index(index_type, own_scope, index_scope)?;
                let (b_idx, b_scope) = b.index(index_type, own_scope, index_scope)?;

                Some((
                    TypeExpr::Intersection(
                        Box::new(TypeExpr::ScopePortal {
                            expr: Box::new(a_idx),
                            scope: ScopePortal { portal: a_scope },
                        }),
                        Box::new(TypeExpr::ScopePortal {
                            expr: Box::new(b_idx),
                            scope: ScopePortal { portal: b_scope },
                        }),
                    ),
                    ScopePointer::clone(own_scope),
                ))
            }

            Self::Operation { a, b, operator } => {
                let a_normalized = a.normalize(own_scope);
                let b_normalized = b.normalize(own_scope);
                T::operation(&a_normalized, operator, &b_normalized).index(index_type, own_scope, index_scope)
            }

            Self::TypeParameter(param, _infer) => {
                // Was:
                // if let Some((bound, scope)) = own_scope.lookup_bound(param) {
                // But in the case:      <T>                 <C>
                //                       | T['abc'] | ----- | C  |
                //
                // C will get inferred using the (bound of T)['abc'] Even when T is not yet inferred.
                if let Some((inferred, scope)) = own_scope.lookup_inferred(param) {
                    inferred.index(index_type, &scope, index_scope)
                } else {
                    None
                }
            }
            Self::ScopePortal { expr, scope } => expr.index(index_type, &scope.as_scope_portal().portal, index_scope),

            // Resolve the inner access first, then index into whatever it produced, so that
            // chained accesses like `{a: {b: Integer}}['a']['b']` evaluate all the way down.
            // Mirrors how [Self::keyof] treats the same two variants.
            Self::Index { expr, index } => {
                let (inner, inner_scope) = expr.index(index, own_scope, own_scope)?;
                inner.index(index_type, &inner_scope, index_scope)
            }
            Self::KeyOf(expr) => {
                let (keys, keys_scope) = expr.keyof(own_scope)?;
                keys.index(index_type, &keys_scope, index_scope)
            }

            // These can't be indexed.
            Self::NodeSignature(_) => Some((TypeExpr::Any, ScopePointer::clone(own_scope))),
            Self::PortTypes(_) => Some((TypeExpr::Any, ScopePointer::clone(own_scope))),
            Self::Conditional { .. } => Some((TypeExpr::Any, ScopePointer::clone(own_scope))),
            Self::Any => Some((TypeExpr::Any, ScopePointer::clone(own_scope))),
            Self::Never => Some((TypeExpr::Any, ScopePointer::clone(own_scope))),
        }
    }
}
