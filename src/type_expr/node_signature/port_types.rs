//! Port types—lists of input or output types for a node.
//!
//! [`PortTypes`] holds a sequence of port types and an optional variadic type (`...T`).
use crate::{
    scope::ScopePointer,
    r#type::Type,
    type_expr::{AsScopedRef, ParamRef, ScopedTypeRef, TypeExpr, TypeRef},
};
#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
#[cfg(feature = "tsify")]
use tsify::Tsify;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
/// A list of port types, optionally with a variadic type (`...T`).
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct PortTypes<T: Type, R: TypeRef = ParamRef> {
    pub ports: Vec<TypeExpr<T, R>>,
    pub varg: Option<TypeExpr<T, R>>,
}

impl<T: Type, R: TypeRef> PortTypes<T, R> {
    pub fn new() -> Self {
        Self { ports: vec![], varg: None }
    }

    pub fn from_ports(ports: Vec<TypeExpr<T, R>>) -> Self {
        Self { ports, varg: None }
    }

    pub fn with_varg(self, varg: TypeExpr<T, R>) -> Self {
        Self { ports: self.ports, varg: Some(varg) }
    }

    pub fn iter(&self) -> impl Iterator<Item = &TypeExpr<T, R>> {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TypeExpr<T, R>> {
        self.into_iter()
    }

    pub fn max_len(&self) -> usize {
        if self.varg.is_some() { usize::MAX } else { self.ports.len() }
    }

    pub fn get_port_type(&self, port_idx: usize) -> Option<&TypeExpr<T, R>> {
        self.ports.get(port_idx).or(self.varg.as_ref())
    }
}

impl<'a, T: Type, R: TypeRef> IntoIterator for &'a PortTypes<T, R> {
    type Item = &'a TypeExpr<T, R>;
    type IntoIter = std::iter::Chain<std::slice::Iter<'a, TypeExpr<T, R>>, std::option::Iter<'a, TypeExpr<T, R>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.ports.iter().chain(self.varg.iter())
    }
}

impl<'a, T: Type, R: TypeRef> IntoIterator for &'a mut PortTypes<T, R> {
    type Item = &'a mut TypeExpr<T, R>;
    type IntoIter = std::iter::Chain<std::slice::IterMut<'a, TypeExpr<T, R>>, std::option::IterMut<'a, TypeExpr<T, R>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.ports.iter_mut().chain(self.varg.iter_mut())
    }
}

impl<T: Type, R: AsScopedRef<T>> PortTypes<T, R> {
    pub fn normalize(&self, scope: &ScopePointer<T>) -> PortTypes<T, ScopedTypeRef<T>> {
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
