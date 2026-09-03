use crate::{
    scope::ScopePointer,
    r#type::Type,
    type_expr::{
        AsScopedRef, ConstructorParams, ScopedRefView, ScopedTypeExpr, ScopedTypeRef, TypeExpr,
        conditional::Conditional,
    },
};

impl<T: Type, R: AsScopedRef<T>> TypeExpr<T, R> {
    /// Same as [normalize](Self::normalize) but for types that aren't context sensitive.
    pub fn normalize_naive(&self) -> ScopedTypeExpr<T> {
        self.normalize(&ScopePointer::new_root())
    }

    /// Builds a normalized version of `self`.
    ///
    /// **Important:** The resulting expression might contain ScopePortals even if self didn't.
    pub fn normalize(&self, scope: &ScopePointer<T>) -> ScopedTypeExpr<T> {
        let (_, scope) = self.build_uninferred_child_scope(scope);

        match self {
            Self::Any => TypeExpr::Any,
            Self::Never => TypeExpr::Never,

            Self::KeyOf(expr) => {
                let Some((key, key_scope)) = expr.keyof(&scope) else {
                    return TypeExpr::KeyOf(Box::new(expr.normalize(&scope)));
                };
                key.normalize(&key_scope)
            }

            // The operands have to be fully resolved before the operation can be evaluated.
            // As long as they aren't, the operation stays as it is and gets evaluated later.
            Self::Operation { a, b, operator } => match (a.normalize_concrete(&scope), b.normalize_concrete(&scope)) {
                (Some(a_normalized), Some(b_normalized)) => T::operation(&a_normalized, operator, &b_normalized).into(),
                _ => TypeExpr::Operation {
                    a: Box::new(a.normalize(&scope)),
                    operator: operator.clone(),
                    b: Box::new(b.normalize(&scope)),
                },
            },

            // If any of the two types is never, then this type is equivalent to the other type (even if that type is never as well).
            // Similarly if any of the two types is any, the result type is also any.
            // If a and b are equal, then simply return a.
            Self::Union(a, b) => {
                if a == b {
                    return a.normalize(&scope);
                }
                if a.supertype_of(b, &scope, &scope).is_supertype() {
                    return a.normalize(&scope);
                }
                if b.supertype_of(a, &scope, &scope).is_supertype() {
                    return b.normalize(&scope);
                }
                if a.is_never_forever(&scope) {
                    return b.normalize(&scope);
                }
                if b.is_never_forever(&scope) {
                    return a.normalize(&scope);
                }
                if a.is_any_forever(&scope) || b.is_any_forever(&scope) {
                    return TypeExpr::Any;
                }

                TypeExpr::Union(Box::new(a.normalize(&scope)), Box::new(b.normalize(&scope)))
            }

            Self::Intersection(a, b) => {
                if a == b {
                    return a.normalize(&scope);
                }
                if a.supertype_of(b, &scope, &scope).is_supertype() {
                    return b.normalize(&scope);
                }
                if b.supertype_of(a, &scope, &scope).is_supertype() {
                    return a.normalize(&scope);
                }
                if a.is_any_forever(&scope) {
                    return b.normalize(&scope);
                }
                if b.is_any_forever(&scope) {
                    return a.normalize(&scope);
                }
                if a.is_never_forever(&scope) || b.is_never_forever(&scope) {
                    return TypeExpr::Never;
                }
                if let Some((intersection, intersection_scope)) = TypeExpr::intersection(a, b, &scope, &scope) {
                    intersection.normalize(&intersection_scope)
                } else {
                    TypeExpr::Intersection(Box::new(a.normalize(&scope)), Box::new(b.normalize(&scope)))
                }
            }

            Self::Type(t) => TypeExpr::Type(t.clone()),

            Self::Conditional(conditional) => {
                if let Some(distributed) = conditional.distribute(&scope) {
                    return distributed.normalize(&scope);
                };
                TypeExpr::Conditional(Box::new(Conditional {
                    t_test: conditional.t_test.normalize(&scope),
                    t_test_bound: conditional.t_test_bound.normalize(&scope),
                    t_then: conditional.t_then.normalize(&scope),
                    t_else: conditional.t_else.normalize(&scope),
                    infer: conditional.infer.clone(),
                }))
            }

            Self::Constructor { inner, parameters } => {
                if parameters.is_empty() {
                    return TypeExpr::Type(inner.clone());
                }
                let mut normalized_params = ConstructorParams::new();
                for (ident, param) in parameters {
                    normalized_params.insert(ident.clone(), param.normalize(&scope));
                }
                TypeExpr::Constructor { inner: inner.clone(), parameters: normalized_params }
            }

            Self::NodeSignature(sig) => TypeExpr::NodeSignature(Box::new(sig.normalize(&scope))),

            Self::PortTypes(ports) => TypeExpr::PortTypes(Box::new(ports.normalize(&scope))),

            Self::Ref(r) => match r.view() {
                ScopedRefView::Param(param) => {
                    if let Some((inferred, inferred_scope)) = scope.lookup_inferred(&param.param_id) {
                        inferred.normalize(&inferred_scope)
                    } else {
                        TypeExpr::Ref(ScopedTypeRef::Param(*param))
                    }
                }

                ScopedRefView::ScopedExpr { expr, scope: portal } => {
                    // Normalize beforehand so that `normalized_expr.contains_type_param` is checked after params got resolved.
                    let normalized_expr = expr.normalize(portal);
                    // If the portal teleports to the same scope as we're in already it has no effect and can be removed.
                    // if both the portal and the running scope are empty, it is also safe to say that it doesn't have an effect.
                    // If the running scope is not empty but the portal is not, the portal still blocks its contents from the outer scope
                    // And thus shouldn't be removed.
                    if portal == &scope
                        || (portal.is_empty() && scope.is_empty())
                        || !normalized_expr.contains_type_param()
                    {
                        // remove the portal
                        normalized_expr
                    } else {
                        TypeExpr::scope_portal(normalized_expr, ScopePointer::clone(portal))
                    }
                }
            },

            Self::Index { expr, index } => {
                let Some((index_type, index_scope)) = expr.index(index, &scope, &scope) else {
                    return TypeExpr::Index {
                        expr: Box::new(expr.normalize(&scope)),
                        index: Box::new(index.normalize(&scope)),
                    };
                };
                index_type.normalize(&index_scope)
            }
        }
    }

    pub fn normalize_to_type(&self, scope: &ScopePointer<T>) -> Option<T> {
        match self.normalize(scope) {
            TypeExpr::Type(t) => Some(t),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        demo_type::DemoType,
        notation::parse::expr,
        scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
        type_expr::{ScopedTypeRef, TypeExpr},
    };
    use assert_matches::assert_matches;

    #[test]
    fn test_scope_portal_dedup() {
        let mut scope = Scope::new_root();
        scope.define(LocalParamID(0), TypeParameter::default());
        let scope = ScopePointer::new(scope);
        let expr = TypeExpr::<DemoType, _>::scope_portal(TypeExpr::Any, ScopePointer::clone(&scope));
        let normalized = expr.normalize(&scope);
        assert_eq!(normalized, TypeExpr::Any);
    }

    #[test]
    fn test_scope_portal_no_dedup() {
        let mut portal_scope = Scope::new_root();
        portal_scope.define(LocalParamID(0), TypeParameter::default());
        let portal_scope = ScopePointer::new(portal_scope);
        let expr =
            TypeExpr::<DemoType, _>::scope_portal(TypeExpr::param(LocalParamID(0)), ScopePointer::clone(&portal_scope));
        let normalized = expr.normalize(&ScopePointer::new_root());
        assert_matches!(normalized, TypeExpr::Ref(ScopedTypeRef::ScopedExpr { .. }));
    }

    #[test]
    fn test_normalize_conditional() {
        let conditional = expr("(Unit|String) extends Unit ? Never : String");
        assert_eq!(expr("String"), conditional.normalize_naive());
    }

    // #[test]
    // fn test_normalize_operation() {
    //     let operation = expr("Any * Any");
    //     let normalized = operation.normalize_naive();
    //     dbg!(&normalized);
    //     // assert_eq!(normalized, expr("() -> ()"));
    // }
}
