//! Port types—lists of input or output types for a node.
//!
//! [`PortTypes`] holds a sequence of port types and an optional variadic type (`...T`).
use crate::{
    scope::ScopePointer,
    r#type::Type,
    type_expr::{AsScopePortal, ScopePortal, TypeExpr, TypeExprScope, Unscoped},
};
#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
#[cfg(feature = "tsify")]
use tsify::Tsify;

#[derive(Debug, Clone, PartialEq)]
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
/// A list of port types, optionally with a variadic type (`...T`).
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct PortTypes<T: Type, S: TypeExprScope = Unscoped> {
    pub ports: Vec<TypeExpr<T, S>>,
    pub varg: Option<TypeExpr<T, S>>,
}

impl<T: Type, S: TypeExprScope> PortTypes<T, S> {
    pub fn new() -> Self {
        Self { ports: vec![], varg: None }
    }

    pub fn from_ports(ports: Vec<TypeExpr<T, S>>) -> Self {
        Self { ports, varg: None }
    }

    pub fn with_varg(self, varg: TypeExpr<T, S>) -> Self {
        Self { ports: self.ports, varg: Some(varg) }
    }

    pub fn iter(&self) -> impl Iterator<Item = &TypeExpr<T, S>> {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TypeExpr<T, S>> {
        self.into_iter()
    }

    pub fn max_len(&self) -> usize {
        if self.varg.is_some() { usize::MAX } else { self.ports.len() }
    }

    pub fn get_port_type(&self, port_idx: usize) -> Option<&TypeExpr<T, S>> {
        self.ports.get(port_idx).or(self.varg.as_ref())
    }
}

impl<'a, T: Type, S: TypeExprScope> IntoIterator for &'a PortTypes<T, S> {
    type Item = &'a TypeExpr<T, S>;
    type IntoIter = std::iter::Chain<std::slice::Iter<'a, TypeExpr<T, S>>, std::option::Iter<'a, TypeExpr<T, S>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.ports.iter().chain(self.varg.iter())
    }
}

impl<'a, T: Type, S: TypeExprScope> IntoIterator for &'a mut PortTypes<T, S> {
    type Item = &'a mut TypeExpr<T, S>;
    type IntoIter = std::iter::Chain<std::slice::IterMut<'a, TypeExpr<T, S>>, std::option::IterMut<'a, TypeExpr<T, S>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.ports.iter_mut().chain(self.varg.iter_mut())
    }
}

impl<T: Type, S: AsScopePortal<T>> PortTypes<T, S> {
    pub fn normalize(&self, scope: &ScopePointer<T>) -> PortTypes<T, ScopePortal<T>> {
        PortTypes {
            ports: self.ports.clone().into_iter().map(|port| port.normalize(scope)).collect(),
            varg: self.varg.clone().map(|varg| varg.normalize(scope)),
        }
    }
}

impl<T: Type> Default for PortTypes<T> {
    fn default() -> Self {
        Self { ports: vec![], varg: None }
    }
}
