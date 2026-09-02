use crate::{
    Type,
    scope::{LocalParamID, type_parameter::TypeParameter},
    type_expr::{ParamRef, TypeRef},
};
use std::{
    collections::BTreeMap,
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// Wrapper for BtreeMap
///
/// Exists so that it can implement traits like [std::str::FromStr]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "T: Serialize, T::Operator: Serialize, R: Serialize",
        deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, R: Deserialize<'de>"
    ))
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, R: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct TypeParameters<T: Type, R: TypeRef = ParamRef>(pub BTreeMap<LocalParamID, TypeParameter<T, R>>);

impl<T: Type, R: TypeRef> Deref for TypeParameters<T, R> {
    type Target = BTreeMap<LocalParamID, TypeParameter<T, R>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Type, R: TypeRef> DerefMut for TypeParameters<T, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Type, R: TypeRef> Default for TypeParameters<T, R> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<T: Type, R: TypeRef> From<BTreeMap<LocalParamID, TypeParameter<T, R>>> for TypeParameters<T, R> {
    fn from(map: BTreeMap<LocalParamID, TypeParameter<T, R>>) -> Self {
        Self(map)
    }
}

impl<T: Type, R: TypeRef> FromIterator<(LocalParamID, TypeParameter<T, R>)> for TypeParameters<T, R> {
    fn from_iter<I: IntoIterator<Item = (LocalParamID, TypeParameter<T, R>)>>(iter: I) -> Self {
        Self(iter.into_iter().collect::<BTreeMap<_, _>>())
    }
}

impl<T: Type, R: TypeRef> IntoIterator for TypeParameters<T, R> {
    type Item = (LocalParamID, TypeParameter<T, R>);
    type IntoIter = std::collections::btree_map::IntoIter<LocalParamID, TypeParameter<T, R>>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T: Type, R: TypeRef> IntoIterator for &'a TypeParameters<T, R> {
    type Item = (&'a LocalParamID, &'a TypeParameter<T, R>);
    type IntoIter = std::collections::btree_map::Iter<'a, LocalParamID, TypeParameter<T, R>>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
