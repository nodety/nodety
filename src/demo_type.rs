//! # Demo Type System
//! This is a demo type system that is used primarily internally for testing.
//! But it also serves as reference implementation of the Type trait.
use crate::{
    r#type::Type,
    type_expr::{ConcreteTypeExpr, ConstructorParams, OperandRef, TypeExpr, TypeRef},
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
    /// An integer literal like `3`, a subtype of `Integer`.
    IntegerLiteral(i64),
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
    /// Effect of nodes whose outputs don't only depend on their inputs.
    Nondeterministic,
    /// Effect of nodes that may fail.
    Fallible,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub enum DemoOperator {
    Multiplication,
    Division,
    Addition,
    Subtraction,
}

impl Type for DemoType {
    type Operator = DemoOperator;

    fn common_supertype(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::String(_), Self::String(_)) => Some(Self::String(None)),
            (Self::Integer | Self::IntegerLiteral(_), Self::Integer | Self::IntegerLiteral(_)) => Some(Self::Integer),
            _ => None,
        }
    }

    fn supertype_of(&self, child: &Self) -> bool {
        match (self, child) {
            // Special types
            (Self::Unit, Self::Unit) => true,

            // Numbers
            (Self::Integer, Self::Integer | Self::IntegerLiteral(_)) => true,
            (Self::IntegerLiteral(a), Self::IntegerLiteral(b)) => a == b,
            (Self::Float, Self::Float) => true,

            // Strings
            (Self::String(None), Self::String(_)) => true,
            (Self::String(Some(a)), Self::String(Some(b))) => a == b,

            (Self::Boolean, Self::Boolean) => true,

            (Self::Record, Self::Record) => true,

            (Self::Array, Self::Array) => true,

            // Interfaces
            (Self::Comparable, Self::Comparable | Self::Integer | Self::IntegerLiteral(_) | Self::Float) => true,
            (Self::Countable, Self::Countable | Self::Array | Self::Record) => true,
            (Self::Sortable, Self::Sortable) => true,

            // SI
            (Self::SI(a, a_scale), Self::SI(b, b_scale)) => a == b && a_scale == b_scale,
            (Self::AnySI, Self::AnySI | Self::SI(_, _)) => true,

            // Effects
            (Self::Nondeterministic, Self::Nondeterministic) => true,
            (Self::Fallible, Self::Fallible) => true,
            (_, _) => false,
        }
    }

    fn operation<R: OperandRef<Self>>(
        a: &TypeExpr<Self, R>,
        operator: &DemoOperator,
        b: &TypeExpr<Self, R>,
    ) -> TypeExpr<Self, R> {
        match (a, operator, b) {
            (
                TypeExpr::Type(Self::SI(a, a_scale)),
                DemoOperator::Multiplication,
                TypeExpr::Type(Self::SI(b, b_scale)),
            ) => TypeExpr::Type(Self::SI(a.multiply(b), a_scale * b_scale)),
            (TypeExpr::Type(Self::SI(a, a_scale)), DemoOperator::Division, TypeExpr::Type(Self::SI(b, b_scale))) => {
                TypeExpr::Type(Self::SI(a.divide(b), a_scale / b_scale))
            }
            (a, DemoOperator::Addition | DemoOperator::Subtraction, b) => {
                let sign = if *operator == DemoOperator::Addition { 1 } else { -1 };
                match (Linear::of(a), Linear::of(b)) {
                    (Some(a), Some(b)) => a.combine(sign, b).map_or(TypeExpr::Type(Self::Integer), Linear::canonical),
                    _ => TypeExpr::Never,
                }
            }
            _ => TypeExpr::Never,
        }
    }

    fn key_type<R: TypeRef>(&self, fields: &ConstructorParams<Self, R>) -> ConcreteTypeExpr<Self> {
        match self {
            Self::Record => {
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
            Self::Array => TypeExpr::Type(Self::Integer),
            _ => TypeExpr::Never,
        }
    }

    fn index<R: TypeRef>(
        &self,
        fields: &ConstructorParams<Self, R>,
        index: &ConcreteTypeExpr<Self>,
    ) -> TypeExpr<Self, R> {
        match (self, index) {
            (Self::Record, index) => {
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

            (Self::Array, index) => {
                if !matches!(index, TypeExpr::Type(Self::Integer)) {
                    return TypeExpr::Type(Self::Unit);
                }
                let Some(t) = fields.get("elements_type") else {
                    return TypeExpr::Type(Self::Unit);
                };
                t.clone()
            }
            _ => TypeExpr::Any,
        }
    }

    fn optional_in_constructor(&self) -> bool {
        matches!(self, Self::Unit)
    }
}

/// `Σ coefficient · parameter + constant`, the reference for computing with rigid parameters: [DemoType]'s integer
/// addition and subtraction keep operations over rigid parameters in this form.
struct Linear<R> {
    /// No coefficient is 0.
    terms: BTreeMap<R, i64>,
    /// `None` for a non literal `Integer`, which absorbs everything.
    constant: Option<i64>,
}

impl<R: OperandRef<DemoType>> Linear<R> {
    fn of(expr: &TypeExpr<DemoType, R>) -> Option<Self> {
        match expr {
            TypeExpr::Type(DemoType::IntegerLiteral(literal)) => {
                Some(Self { terms: BTreeMap::new(), constant: Some(*literal) })
            }
            TypeExpr::Type(DemoType::Integer) => Some(Self { terms: BTreeMap::new(), constant: None }),
            TypeExpr::Ref(r) => Some(Self { terms: [(r.clone(), 1)].into(), constant: Some(0) }),
            TypeExpr::Operation { a, operator: operator @ (DemoOperator::Addition | DemoOperator::Subtraction), b } => {
                let sign = if *operator == DemoOperator::Addition { 1 } else { -1 };
                Self::of(a)?.combine(sign, Self::of(b)?)
            }
            _ => None,
        }
    }

    /// `self + sign · other`. `None` on an overflow.
    fn combine(mut self, sign: i64, other: Self) -> Option<Self> {
        for (term, coefficient) in other.terms {
            let sum = self.terms.get(&term).copied().unwrap_or(0).checked_add(coefficient.checked_mul(sign)?)?;
            self.terms.insert(term, sum);
        }
        self.terms.retain(|_, coefficient| *coefficient != 0);
        self.constant = match (self.constant, other.constant) {
            (Some(a), Some(b)) => Some(a.checked_add(b.checked_mul(sign)?)?),
            _ => None,
        };
        Some(self)
    }

    /// The positive terms in order, then the negative ones, then the constant: `N + N - M + 1`.
    fn canonical(self) -> TypeExpr<DemoType, R> {
        let constant = match self.constant {
            None => return TypeExpr::Type(DemoType::Integer),
            Some(constant) => constant,
        };
        let literal = |literal: i64| TypeExpr::Type(DemoType::IntegerLiteral(literal));
        let operation = |a, operator, b| TypeExpr::Operation { a: Box::new(a), operator, b: Box::new(b) };
        let mut positive =
            self.terms.iter().filter(|(_, c)| **c > 0).flat_map(|(r, c)| (0..*c).map(move |_| r.clone()));
        let mut expr = match positive.next() {
            Some(first) => {
                positive.fold(TypeExpr::Ref(first), |e, r| operation(e, DemoOperator::Addition, TypeExpr::Ref(r)))
            }
            None => literal(constant),
        };
        for (r, coefficient) in self.terms.iter().filter(|(_, c)| **c < 0) {
            for _ in 0..-coefficient {
                expr = operation(expr, DemoOperator::Subtraction, TypeExpr::Ref(r.clone()));
            }
        }
        match self.terms.values().any(|c| *c > 0) {
            true if constant > 0 => operation(expr, DemoOperator::Addition, literal(constant)),
            true if constant < 0 => operation(expr, DemoOperator::Subtraction, literal(-constant)),
            _ => expr,
        }
    }
}
