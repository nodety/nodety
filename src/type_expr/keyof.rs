use crate::{
    Type, TypeExpr,
    scope::ScopePointer,
    type_expr::{AsScopedRef, ScopedRefView, ScopedTypeExpr},
};

impl<T: Type, R: AsScopedRef<T>> TypeExpr<T, R> {
    /// Returns the keys of the constructor fields if this is a constructor or is inferred to be a constructor. None otherwise.
    /// The returned expression is guaranteed not to contain any context sensitive types.
    /// # Returns
    /// `None` if
    /// - the key type is unknown due to uninferred vars.
    /// - Intersection or Union with distinct scopes.
    pub fn keyof(&self, scope: &ScopePointer<T>) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        // Normalize here so Index, keyof and TypeParameter don't need to get handled by this function.
        match self {
            Self::Type(inst) => Some((inst.key_type::<R>(None).into(), ScopePointer::clone(scope))),

            Self::Constructor { inner, parameters } => {
                // Parameters should have been normalized by caller
                Some((inner.key_type(Some(parameters)).into(), ScopePointer::clone(scope)))
            }

            // See tsReference.ts
            // @Todo: Test this
            // keyof (A|B) = keyof A & keyof B
            // This is not exactly what typescript does but close enough.
            Self::Union(a, b) => {
                let (keyof_a, keyof_a_scope) = a.keyof(scope)?;
                let (keyof_b, keyof_b_scope) = b.keyof(scope)?;
                if keyof_a.is_never_forever(&keyof_a_scope) {
                    return Some((keyof_b, keyof_b_scope));
                }
                if keyof_b.is_never_forever(&keyof_b_scope) {
                    return Some((keyof_a, keyof_a_scope));
                }
                TypeExpr::intersection(&keyof_a, &keyof_b, &keyof_a_scope, &keyof_b_scope)
            }

            // See tsReference.ts
            // @todo test this
            Self::Intersection(a, b) => {
                if a.is_never_forever(scope) || b.is_never_forever(scope) {
                    return Some((TypeExpr::Never, ScopePointer::clone(scope)));
                }
                let (keyof_a, keyof_a_scope) = a.keyof(scope)?;
                let (keyof_b, keyof_b_scope) = b.keyof(scope)?;
                Some((
                    TypeExpr::Union(
                        Box::new(TypeExpr::scope_portal(keyof_a, keyof_a_scope)),
                        Box::new(TypeExpr::scope_portal(keyof_b, keyof_b_scope)),
                    ),
                    ScopePointer::clone(scope),
                ))
            }

            Self::Operation { a, b, operator } => {
                let a_normalized = a.normalize_concrete(scope)?;
                let b_normalized = b.normalize_concrete(scope)?;
                T::operation(&a_normalized, operator, &b_normalized).keyof(scope)
            }

            Self::Ref(r) => match r.view() {
                // Was:
                // if let Some((bound, scope)) = scope.lookup_bound(param) {
                // But in the case:      <T>                 <C>
                //                       |   keyof T | ----- | C  |
                //
                // C will get inferred using the keyof(bound of T) Even when T is not yet inferred.
                ScopedRefView::Param(param) => {
                    let (inferred, scope) = scope.lookup_inferred(&param.param_id)?;
                    inferred.keyof(&scope)
                }
                ScopedRefView::ScopedExpr { expr, scope: portal } => expr.keyof(portal),
            },

            Self::KeyOf(expr) => expr.keyof(scope),

            Self::Index { expr, index } => {
                let (index_type, index_scope) = expr.index(index, scope, scope)?;
                index_type.keyof(&index_scope)
            }

            Self::Any => Some((T::keyof_any().into(), ScopePointer::clone(scope))),
            Self::NodeSignature(node_signature) => {
                // Customized behavior (defaults to never)
                Some((T::keyof_node_signature(node_signature).into(), ScopePointer::clone(scope)))
            }
            Self::PortTypes(_) => Some((TypeExpr::Never, ScopePointer::clone(scope))),
            // @todo
            Self::Conditional { .. } => Some((TypeExpr::Never, ScopePointer::clone(scope))),
            Self::Never => Some((TypeExpr::Never, ScopePointer::clone(scope))),
        }
    }
}
