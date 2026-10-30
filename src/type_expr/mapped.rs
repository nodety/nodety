use crate::{
    scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{AsScopedRef, ConstructorParams, ScopedRefView, ScopedTypeExpr, TypeExpr, TypeRef},
};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// A homomorphic mapped type: `{[K in keyof source]: value}`.
///
/// Has the fields of `source`, each with the type `value` takes when `key_param` is the key type of that field.
/// The key type of a field is [Type::key_type] of a constructor with only that field.
/// Mapping keeps the inner type of `source` and distributes over unions in it, like typescript does.
///
/// As long as `source` isn't known yet, the mapped type stays as it is. If `source` is a type parameter that `value`
/// doesn't reference, inference takes the type flowing into the mapped type as a candidate for it, because only the
/// keys of the source matter. `<C>({[K in keyof C]: Comparable}) -> (C)` takes records of comparable values.
///
/// Example: `{[K in keyof T]: K extends 'points' ? Integer : T[K]}` is `T` with the field `points` replaced by `Integer`.
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
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct Mapped<T: Type, R: TypeRef> {
    /// Only visible inside `value`, where it shadows parameters of the same id.
    pub key_param: LocalParamID,
    pub source: TypeExpr<T, R>,
    pub value: TypeExpr<T, R>,
}

impl<T: Type, R: TypeRef> Mapped<T, R> {
    pub(crate) fn try_map_refs_mut<RO: TypeRef, E, F>(self, mapper: &mut F) -> Result<Mapped<T, RO>, E>
    where
        F: FnMut(R) -> Result<RO, E>,
    {
        Ok(Mapped {
            key_param: self.key_param,
            source: self.source.try_map_refs_mut(mapper)?,
            value: self.value.try_map_refs_mut(mapper)?,
        })
    }
}

impl<T: Type, R: AsScopedRef<T>> Mapped<T, R> {
    /// Builds the mapped type.
    ///
    /// # Returns
    /// The mapped type in `scope`, or `None` if `source` isn't known yet.
    pub fn evaluate(&self, scope: &ScopePointer<T>) -> Option<ScopedTypeExpr<T>> {
        self.map_fields(&self.source.normalize(scope), scope, scope)
    }

    /// The scope `value` is read in: `scope` with `key_param` defined, and inferred to `key` if given.
    pub(crate) fn value_scope(&self, scope: &ScopePointer<T>, key: Option<ScopedTypeExpr<T>>) -> ScopePointer<T> {
        let mut value_scope = Scope::new_child(scope);
        value_scope.define(self.key_param, TypeParameter::default());
        if let Some(key) = key {
            value_scope.infer(&self.key_param, key, ScopePointer::new_root()).expect("the key param was just defined");
        }
        ScopePointer::new(value_scope)
    }

    /// `source` is the normalized source in `source_scope`. The result lives in `scope`, the scope of `self`.
    fn map_fields(
        &self,
        source: &ScopedTypeExpr<T>,
        source_scope: &ScopePointer<T>,
        scope: &ScopePointer<T>,
    ) -> Option<ScopedTypeExpr<T>> {
        match source {
            TypeExpr::Constructor { inner, parameters } => {
                let parameters = parameters
                    .iter()
                    .map(|(field, field_type)| {
                        let field_only: ConstructorParams<T, _> = [(field.clone(), field_type.clone())].into();
                        let key = inner.key_type(&field_only).into_scoped();
                        let value = TypeExpr::scope_portal(
                            self.value.clone().into_scoped(),
                            self.value_scope(scope, Some(key)),
                        );
                        (field.clone(), value)
                    })
                    .collect();
                Some(TypeExpr::Constructor { inner: inner.clone(), parameters })
            }
            TypeExpr::Union(a, b) => Some(TypeExpr::Union(
                Box::new(self.map_fields(a, source_scope, scope)?),
                Box::new(self.map_fields(b, source_scope, scope)?),
            )),
            TypeExpr::Type(_) | TypeExpr::Any | TypeExpr::Never => Some(source.clone()),
            TypeExpr::Ref(r) => match r.view() {
                ScopedRefView::Param(param) => {
                    let (inferred, inferred_scope) = source_scope.lookup_inferred(&param.param_id)?;
                    self.map_fields(&inferred.normalize(&inferred_scope), &inferred_scope, scope)
                }
                ScopedRefView::ScopedExpr { expr, scope: portal } => self.map_fields(expr, portal, scope),
            },
            _ => None,
        }
    }
}
