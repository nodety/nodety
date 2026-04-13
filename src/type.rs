use crate::type_expr::{ScopePortal, ScopedTypeExpr, TypeExpr, node_signature::NodeSignature};
use std::{collections::BTreeMap, fmt::Debug};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// Never type for operators.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(type = "never"))]
#[derive(Clone, Debug, PartialEq)]
pub enum NoOperator {
    // Never add a variant here!
}

/// # Type
/// The Type trait is at the heart of a node type system.
/// For a reference implementation have a look at the [`demo_type`](crate::demo_type) module.
///
/// ## Nodety's type model
/// [Type]s are atomic. That means that they are always leaf nodes in a type expression.
/// But you will probably want to make your types generic to express things like `Map<K, V>`
/// where Array is one of your types. To represent this, type expressions have [Constructor][TypeExpr::Constructor]s.
/// A constructor contains a Type and a map of parameters which are themselves type expressions.
/// You can use them to represent two scenarios:
/// ### Generic parameters
/// ```
/// # use maplit::btreemap;
/// # use nodety::{Type, TypeExpr, NoOperator};
/// // Let's say your type structure looks something like this:
/// #[derive(Clone, Debug, PartialEq)]
/// pub enum MyType {
///   Map,
///   Integer,
///   String
/// }
///
/// impl Type for MyType {
///   type Operator = NoOperator;
/// }
///
/// // Then you can represent the type `Map<Integer, String>` as follows:
/// let t_map = TypeExpr::<MyType>::Constructor {
///   inner: MyType::Map,
///   parameters: btreemap! {
///     "K".into() => TypeExpr::Type(MyType::Integer),
///     "V".into() => TypeExpr::Type(MyType::String),
///   },
/// };
/// ```
/// ### Records / Objects
/// You can represent records / objects in the same way.
/// Let's say you want to represent the type `{ property_a: Integer, property_b: String, property_c: String }`.
/// ```
/// # use maplit::btreemap;
/// # use nodety::{Type, TypeExpr, NoOperator};
/// #[derive(Clone, Debug, PartialEq)]
/// pub enum MyType {
///   Record,
///   Integer,
///   String
/// }
///
/// impl Type for MyType {
///   type Operator = NoOperator;
/// }
///
/// let t_map = TypeExpr::<MyType>::Constructor {
///   inner: MyType::Record,
///   parameters: btreemap! {
///     "property_a".into() => TypeExpr::Type(MyType::Integer),
///     "property_b".into() => TypeExpr::Type(MyType::String),
///     "property_c".into() => TypeExpr::Type(MyType::String),
///   },
/// };
/// ```
///
/// ## Constructor subtyping
///
/// A Constructor A is considered a supertype of another constructor B if A's inner type is a supertype of B's inner type.
/// And A has the same or more parameters than B. And A's parameters are each supertypes of B's parameters.
///
/// ## Intersections
///
/// The intersection between two constructors A and B exists if A's inner type is equal to B's inner type.
/// Then the intersection will get all the parameters of A and B. And the parameters that are present in both
/// A and B will be the intersection of both.
pub trait Type: Sized + Clone + Debug + PartialEq {
    /// Used for custom operators. If no custom operators are used, set this to [NoOperator].
    type Operator: PartialEq + Debug + Clone;

    /// The heart of any type system.
    ///
    /// Determines whether `self` is a supertype of `child` —i.e., the child is at least as useful as
    /// the parent (to borrow jonhoo's phrasing). `false` otherwise.
    ///
    /// All types MUST BE reflexive, i.e. `self.supertype_of(self) == true`.
    ///
    /// In a node editor, connections are valid if their target type is a supertype of the source type.
    fn supertype_of(&self, child: &Self) -> bool {
        self == child
    }

    /// # Custom operators
    /// Some type systems may want to extends the set of operations on their types.
    /// A good example for this is the SI unit type system. It may define an SI type which knows about a value's unit.
    /// Then it may define the mathematical operators for SI units.
    /// Then something like the following becomes possible:
    ///
    /// `<A extends SI, B extends SI>(A, B) -> (A / B)`
    ///
    /// Here, when A is `[m]` and B is `[s]` then a custom division operation might return `[m/s]`.
    ///
    /// Checkout the [SIUnit](crate::demo_type::SIUnit) type for a reference implementation with operators.
    ///
    /// # Returns
    /// the result type of the operation, or `Never` if the operation is not permitted.
    fn operation(
        _a: &TypeExpr<Self, ScopePortal<Self>>,
        _operator: &Self::Operator,
        _b: &TypeExpr<Self, ScopePortal<Self>>,
    ) -> ScopedTypeExpr<Self> {
        TypeExpr::Never
    }

    /// Used to evaluate keyof expressions.
    ///
    /// # Parameters
    /// - `fields`: if `self` is wrapped in a constructor then these are the normalized parameters.
    ///
    /// # Returns
    /// The key type of `self` or [TypeExpr::Never] if it has no key type.
    fn key_type(&self, _fields: Option<&BTreeMap<String, ScopedTypeExpr<Self>>>) -> ScopedTypeExpr<Self> {
        TypeExpr::Never
    }

    /// Used to evaluate index expressions.
    ///
    /// # Parameters
    /// - `fields`: if `self` is wrapped in a constructor then these are the normalized parameters.
    ///
    /// # Returns
    /// The type representing `self[index]`. If `self` cannot be indexed with the index type, then [TypeExpr::Any] could be returned.
    /// At least thats what typescript does. Apply your own judgement.
    fn index(
        &self,
        _fields: Option<&BTreeMap<String, ScopedTypeExpr<Self>>>,
        _index: &ScopedTypeExpr<Self>,
    ) -> ScopedTypeExpr<Self> {
        TypeExpr::Any
    }

    /// Used to evaluate `keyof Any` expressions.
    /// In typescript this evaluates to `string | number | symbol`.
    /// In most other type systems, `never` is probably a safer bet.
    ///
    /// Implement this method if you want to customize this behavior.
    fn keyof_any() -> ScopedTypeExpr<Self> {
        TypeExpr::Never
    }

    /// Same as in ts.
    ///
    /// Overwrite this to customize the behavior.
    fn keyof_node_signature(_node_signature: &NodeSignature<Self, ScopePortal<Self>>) -> ScopedTypeExpr<Self> {
        TypeExpr::Never
    }
}
