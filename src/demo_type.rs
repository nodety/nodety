//! # Demo Type System
//! This is a demo type system that is used primarily internally for testing.
//! But it also serves as reference implementation of the Type trait.
use crate::{
    r#type::Type,
    type_expr::{ScopePortal, ScopedTypeExpr, TypeExpr},
};
#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// # SIUnit
/// Learn more about the SI unit system [here](https://en.wikipedia.org/wiki/International_System_of_Units).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct SIUnit {
    pub s: i16,
    pub m: i16,
    pub kg: i16,
    pub a: i16,
    pub k: i16,
    pub mol: i16,
    pub cd: i16,
}

impl SIUnit {
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            s: self.s + other.s,
            m: self.m + other.m,
            kg: self.kg + other.kg,
            a: self.a + other.a,
            k: self.k + other.k,
            mol: self.mol + other.mol,
            cd: self.cd + other.cd,
        }
    }

    pub fn divide(&self, other: &Self) -> Self {
        Self {
            s: self.s - other.s,
            m: self.m - other.m,
            kg: self.kg - other.kg,
            a: self.a - other.a,
            k: self.k - other.k,
            mol: self.mol - other.mol,
            cd: self.cd - other.cd,
        }
    }
}

/// Demo type system used for testing and demonstration purposes.
#[derive(Debug, Clone, PartialEq)]
pub enum DemoType {
    Integer,
    Float,
    /// Some if representing a string literal like 'hello', None if representing the string type.
    String(Option<String>),
    Boolean,
    Countable,
    Comparable,
    Sortable,
    Unit,
    Record,
    Array,
    SI(SIUnit, f64),
    AnySI,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub enum DemoOperator {
    Multiplication,
    Division,
}

impl Type for DemoType {
    type Operator = DemoOperator;

    fn supertype_of(&self, child: &Self) -> bool {
        match (self, child) {
            // Special types
            (Self::Unit, Self::Unit) => true,

            // Numbers
            (Self::Integer, Self::Integer) => true,
            (Self::Float, Self::Float) => true,

            // Strings
            (Self::String(None), Self::String(_)) => true,
            (Self::String(Some(a)), Self::String(Some(b))) => a == b,

            (Self::Boolean, Self::Boolean) => true,

            (Self::Record, Self::Record) => true,

            (Self::Array, Self::Array) => true,

            // Interfaces
            (Self::Comparable, Self::Comparable | Self::Integer | Self::Float) => true,
            (Self::Countable, Self::Countable | Self::Array | Self::Record) => true,
            (Self::Sortable, Self::Sortable) => true,

            // SI
            (Self::SI(a, a_scale), Self::SI(b, b_scale)) => a == b && a_scale == b_scale,
            (Self::AnySI, Self::AnySI | Self::SI(_, _)) => true,
            (_, _) => false,
        }
    }

    fn operation(
        a: &TypeExpr<Self, ScopePortal<Self>>,
        operator: &DemoOperator,
        b: &TypeExpr<Self, ScopePortal<Self>>,
    ) -> ScopedTypeExpr<Self> {
        match (a, operator, b) {
            (
                TypeExpr::Type(Self::SI(a, a_scale)),
                DemoOperator::Multiplication,
                TypeExpr::Type(Self::SI(b, b_scale)),
            ) => TypeExpr::Type(Self::SI(a.multiply(b), a_scale * b_scale)),
            (TypeExpr::Type(Self::SI(a, a_scale)), DemoOperator::Division, TypeExpr::Type(Self::SI(b, b_scale))) => {
                TypeExpr::Type(Self::SI(a.divide(b), a_scale / b_scale))
            }
            _ => TypeExpr::Never,
        }
    }

    fn key_type(&self, fields: Option<&BTreeMap<String, ScopedTypeExpr<Self>>>) -> ScopedTypeExpr<Self> {
        match (self, fields) {
            (Self::Record, Some(fields)) => {
                let mut keys = fields.keys().cloned().collect::<Vec<String>>();
                let Some(first) = keys.pop() else {
                    return TypeExpr::Never;
                };
                let Some(second) = keys.pop() else {
                    return TypeExpr::Type(Self::String(Some(first)));
                };
                let mut current = TypeExpr::Union(
                    Box::new(TypeExpr::Type(Self::String(Some(first)))),
                    Box::new(TypeExpr::Type(Self::String(Some(second)))),
                );
                while let Some(key) = keys.pop() {
                    current = TypeExpr::Union(Box::new(current), Box::new(TypeExpr::Type(Self::String(Some(key)))))
                }
                current
            }
            #[allow(deprecated)]
            (Self::Array, _) => TypeExpr::Type(Self::Integer),
            _ => TypeExpr::Never,
        }
    }

    fn index(
        &self,
        fields: Option<&BTreeMap<String, ScopedTypeExpr<Self>>>,
        index: &ScopedTypeExpr<Self>,
    ) -> ScopedTypeExpr<Self> {
        match (self, fields, index) {
            (Self::Record, Some(fields), index) => {
                let mut union_str_literals: Vec<Option<&String>> = vec![];
                index.traverse_union_non_context_sensitive(&mut |expr| {
                    if let TypeExpr::Type(Self::String(Some(literal))) = expr {
                        union_str_literals.push(Some(literal));
                    } else {
                        union_str_literals.push(None);
                    }
                });
                if union_str_literals.is_empty() {
                    return TypeExpr::Any;
                }
                let indexed = union_str_literals.into_iter().map(|literal| {
                    let Some(literal) = literal else {
                        return TypeExpr::Any;
                    };
                    let Some(field) = fields.get(literal) else {
                        return TypeExpr::Any;
                    };
                    field.clone()
                });
                TypeExpr::from_unions(indexed)
            }

            (Self::Array, Some(fields), index) => {
                if !matches!(index, TypeExpr::Type(Self::Integer)) {
                    return TypeExpr::Type(Self::Unit);
                }
                let Some(t) = fields.get(&"elements_type".to_string()) else {
                    return TypeExpr::Type(Self::Unit);
                };
                t.clone()
            }
            _ => TypeExpr::Any,
        }
    }
}
