use super::ScopePointer;
use crate::r#type::Type;
use crate::type_expr::{AsScopedRef, ParamRef, ScopedTypeRef, TypeExpr, TypeRef};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        rename_all = "camelCase",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize, R: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, R: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, R: JsonSchema"))]
/// A generic type parameter with optional bound and default.
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct TypeParameter<T: Type, R: TypeRef = ParamRef> {
    /// Upper bound (e.g. `T extends Comparable`).
    pub bound: Option<TypeExpr<T, R>>,
    /// Default when not inferred (e.g. `T = Any`).
    pub default: Option<TypeExpr<T, R>>,
}

impl<T: Type, R: AsScopedRef<T>> TypeParameter<T, R> {
    /// Normalizes type parameters in bound and default. Returns `None` if normalization fails (e.g. uninferred vars when `any_on_uninferred` is false).
    pub fn normalize(&self, scope: &ScopePointer<T>) -> TypeParameter<T, ScopedTypeRef<T>> {
        TypeParameter {
            bound: self.bound.clone().map(|bound| bound.normalize(scope)),
            default: self.default.clone().map(|default| default.normalize(scope)),
        }
    }
}

impl<T: Type, R: TypeRef> Default for TypeParameter<T, R> {
    fn default() -> Self {
        Self { bound: None, default: None }
    }
}
