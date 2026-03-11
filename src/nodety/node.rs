//! Node and TypeHints types for the nodety graph.

use crate::{
    scope::LocalParamID,
    r#type::Type,
    type_expr::{TypeExpr, TypeExprScope, Unscoped, node_signature::NodeSignature},
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
        serialize = "T: Serialize, T::Operator: Serialize, S: Serialize",
        deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, S: Deserialize<'de>"
    ))
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, S: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone, PartialEq)]
pub struct TypeHints<T: Type, S: TypeExprScope = Unscoped>(pub BTreeMap<LocalParamID, TypeExpr<T, S>>);

impl<T: Type, S: TypeExprScope> Default for TypeHints<T, S> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<T: Type, S: TypeExprScope> Deref for TypeHints<T, S> {
    type Target = BTreeMap<LocalParamID, TypeExpr<T, S>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Type, S: TypeExprScope> From<BTreeMap<LocalParamID, TypeExpr<T, S>>> for TypeHints<T, S> {
    fn from(map: BTreeMap<LocalParamID, TypeExpr<T, S>>) -> Self {
        Self(map)
    }
}

impl<T: Type, S: TypeExprScope> From<TypeHints<T, S>> for BTreeMap<LocalParamID, TypeExpr<T, S>> {
    fn from(hints: TypeHints<T, S>) -> Self {
        hints.0
    }
}

impl<T: Type, S: TypeExprScope> FromIterator<(LocalParamID, TypeExpr<T, S>)> for TypeHints<T, S> {
    fn from_iter<I: IntoIterator<Item = (LocalParamID, TypeExpr<T, S>)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a, T: Type, S: TypeExprScope> IntoIterator for &'a TypeHints<T, S> {
    type Item = (&'a LocalParamID, &'a TypeExpr<T, S>);
    type IntoIter = std::collections::btree_map::Iter<'a, LocalParamID, TypeExpr<T, S>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T: Type, S: TypeExprScope> IntoIterator for TypeHints<T, S> {
    type Item = (LocalParamID, TypeExpr<T, S>);
    type IntoIter = std::collections::btree_map::IntoIter<LocalParamID, TypeExpr<T, S>>;

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
            serialize = "T: Serialize, T::Operator: Serialize, S: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, S: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, S: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone)]
pub struct Node<T: Type, S: TypeExprScope = Unscoped> {
    pub signature: NodeSignature<T, S>,
    /// Node index of the parent node if there is one.
    #[cfg_attr(feature = "json-schema", schemars(with = "usize"))]
    #[cfg_attr(feature = "serde", serde(with = "node_index_serde"))]
    pub parent: Option<NodeIndex>,
    /// These will get inferred directly before inferring anything else. Setting
    /// this is required only when inference is ambiguous. Aka rusts "type annotations needed".
    pub type_hints: TypeHints<T, S>,
}

impl<T: Type> Node<T, Unscoped> {
    pub fn new(signature: NodeSignature<T, Unscoped>) -> Self {
        Self { signature, parent: None, type_hints: TypeHints::default() }
    }

    pub fn new_child(signature: NodeSignature<T, Unscoped>, parent: NodeIndex) -> Self {
        Self { signature, parent: Some(parent), type_hints: TypeHints::default() }
    }

    pub fn with_type_hints(self, type_hints: BTreeMap<LocalParamID, TypeExpr<T, Unscoped>>) -> Self {
        Self { type_hints: type_hints.into(), ..self }
    }
}
