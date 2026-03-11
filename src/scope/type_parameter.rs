use super::ScopePointer;
use crate::r#type::Type;
use crate::type_expr::{ScopePortal, TypeExpr, TypeExprScope, Unscoped};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(rename_all = "camelCase", bound(
        serialize = "T: Serialize, T::Operator: Serialize, S: Serialize",
        deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, S: Deserialize<'de>"
    ))
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, S: JsonSchema"))]
/// A generic type parameter with optional bound and default.
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct TypeParameter<T: Type, S: TypeExprScope = Unscoped> {
    /// Upper bound (e.g. `T extends Comparable`).
    pub bound: Option<TypeExpr<T, S>>,
    /// Default when not inferred (e.g. `T = Any`).
    pub default: Option<TypeExpr<T, S>>,
}

impl<T: Type> TypeParameter<T, ScopePortal<T>> {
    /// Normalizes type parameters in bound and default. Returns `None` if normalization fails (e.g. uninferred vars when `any_on_uninferred` is false).
    pub fn normalize(&self, scope: &ScopePointer<T>) -> TypeParameter<T, ScopePortal<T>> {
        Self {
            bound: self.bound.clone().map(|bound| bound.normalize(scope)),
            default: self.default.clone().map(|default| default.normalize(scope)),
        }
    }
}

impl<T: Type, S: TypeExprScope> Default for TypeParameter<T, S> {
    fn default() -> Self {
        Self { bound: None, default: None }
    }
}
