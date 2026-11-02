use crate::{
    scope::ScopePointer,
    r#type::Type,
    type_expr::{AsScopedRef, ScopedTypeExpr, ScopedTypeRef, TypeExpr},
};

impl<T: Type, R: AsScopedRef<T>> TypeExpr<T, R> {
    /// Evaluates `readonly self`, see [TypeExpr::Readonly].
    ///
    /// # Returns
    /// `None` if it is unknown as long as a type parameter isn't inferred, e.g. for `readonly T`.
    /// Parts of the result may still wait for type parameters: `readonly {a: T}` is `{a: readonly T}`.
    pub fn readonly(&self, scope: &ScopePointer<T>) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        match self.normalize(scope).into_readonly() {
            TypeExpr::Readonly(_) => None,
            readonly => Some((readonly, ScopePointer::clone(scope))),
        }
    }
}

impl<T: Type> ScopedTypeExpr<T> {
    /// Replaces every mutable type in the normalized `self` by its [read-only counterpart](Type::readonly_counterpart).
    /// What can't be resolved yet stays wrapped in [TypeExpr::Readonly].
    pub(crate) fn into_readonly(self) -> Self {
        match self {
            Self::Type(t) => Self::Type(t.readonly_counterpart().unwrap_or(t)),
            Self::Constructor { inner, parameters } => Self::Constructor {
                inner: inner.readonly_counterpart().unwrap_or(inner),
                parameters: parameters.into_iter().map(|(ident, param)| (ident, param.into_readonly())).collect(),
            },
            Self::Union(a, b) => Self::Union(Box::new(a.into_readonly()), Box::new(b.into_readonly())),
            Self::Intersection(a, b) => Self::Intersection(Box::new(a.into_readonly()), Box::new(b.into_readonly())),
            Self::Bounded { lower, upper } => Self::bounded(lower.into_readonly(), upper.into_readonly()),
            Self::Ref(ScopedTypeRef::ScopedExpr { expr, scope }) => Self::scope_portal(expr.into_readonly(), scope),
            // Node signatures are values that can't be written to.
            Self::Any | Self::Never | Self::NodeSignature(_) | Self::PortTypes(_) | Self::Readonly(_) => self,
            Self::Ref(ScopedTypeRef::Param(_))
            | Self::KeyOf(_)
            | Self::Index { .. }
            | Self::Operation { .. }
            | Self::Conditional(_) => Self::Readonly(Box::new(self)),
        }
    }
}
