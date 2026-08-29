//! Node and TypeHints types for the nodety graph.

use crate::{
    scope::LocalParamID,
    r#type::Type,
    type_expr::{ParamRef, TypeExpr, TypeRef, node_signature::NodeSignature},
};
use petgraph::graph::NodeIndex;
use std::{collections::BTreeMap, ops::Deref};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// Type hints for pre-inference annotations (e.g. `T = Integer, U = String`).
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
#[derive(Debug, Clone, PartialEq)]
pub struct TypeHints<T: Type, R: TypeRef = ParamRef>(pub BTreeMap<LocalParamID, TypeExpr<T, R>>);

impl<T: Type, R: TypeRef> Default for TypeHints<T, R> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<T: Type, R: TypeRef> Deref for TypeHints<T, R> {
    type Target = BTreeMap<LocalParamID, TypeExpr<T, R>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Type, R: TypeRef> From<BTreeMap<LocalParamID, TypeExpr<T, R>>> for TypeHints<T, R> {
    fn from(map: BTreeMap<LocalParamID, TypeExpr<T, R>>) -> Self {
        Self(map)
    }
}

impl<T: Type, R: TypeRef> From<TypeHints<T, R>> for BTreeMap<LocalParamID, TypeExpr<T, R>> {
    fn from(hints: TypeHints<T, R>) -> Self {
        hints.0
    }
}

impl<T: Type, R: TypeRef> FromIterator<(LocalParamID, TypeExpr<T, R>)> for TypeHints<T, R> {
    fn from_iter<I: IntoIterator<Item = (LocalParamID, TypeExpr<T, R>)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a, T: Type, R: TypeRef> IntoIterator for &'a TypeHints<T, R> {
    type Item = (&'a LocalParamID, &'a TypeExpr<T, R>);
    type IntoIter = std::collections::btree_map::Iter<'a, LocalParamID, TypeExpr<T, R>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T: Type, R: TypeRef> IntoIterator for TypeHints<T, R> {
    type Item = (LocalParamID, TypeExpr<T, R>);
    type IntoIter = std::collections::btree_map::IntoIter<LocalParamID, TypeExpr<T, R>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(feature = "serde")]
mod node_index_serde {
    use petgraph::graph::NodeIndex;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<NodeIndex>, s: S) -> Result<S::Ok, S::Error> {
        v.as_ref().map(|i| i.index()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<NodeIndex>, D::Error> {
        Option::<usize>::deserialize(d).map(|o| o.map(NodeIndex::new))
    }
}

/// A node in the nodety graph.
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
#[derive(Debug, Clone)]
pub struct Node<T: Type, R: TypeRef = ParamRef> {
    pub signature: NodeSignature<T, R>,
    /// Node index of the parent node if there is one.
    #[cfg_attr(feature = "json-schema", schemars(with = "usize"))]
    #[cfg_attr(feature = "serde", serde(default, with = "node_index_serde"))]
    #[cfg_attr(feature = "tsify", tsify(type = "number"))]
    pub parent: Option<NodeIndex>,
    /// These will get inferred directly before inferring anything else. Setting
    /// this is required only when inference is ambiguous. Aka rusts "type annotations needed".
    #[cfg_attr(feature = "serde", serde(default))]
    pub type_hints: TypeHints<T, R>,
}

impl<T: Type> Node<T, ParamRef> {
    pub fn new(signature: NodeSignature<T, ParamRef>) -> Self {
        Self { signature, parent: None, type_hints: TypeHints::default() }
    }

    pub fn new_child(signature: NodeSignature<T, ParamRef>, parent: NodeIndex) -> Self {
        Self { signature, parent: Some(parent), type_hints: TypeHints::default() }
    }

    pub fn with_type_hints(self, type_hints: BTreeMap<LocalParamID, TypeExpr<T, ParamRef>>) -> Self {
        Self { type_hints: type_hints.into(), ..self }
    }
}
