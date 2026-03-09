// GlobalParameterId is keyed by Rc pointer identity (ptr_eq / as_ptr),
// so interior mutability of the pointee is irrelevant.
#![allow(clippy::mutable_key_type)]

//! # Nodety
//!
//! Nodety *(aka node type system)* is a lightweight and easy to use library that implements generics, type inference and checking for a user defined type system.
//! It is designed to be used in the context of a visual node editors. But it might be useful in other areas as well.
//!
//! It is inspired by typescript and supports most of its features:
//!
//! **Supported features:**
//! - Conditional types `T extends String ? String : Never` (Eliminate all types from T except for those assignable to String)
//! - Function types (in nodety called NodeSignatures) `<T>(T) -> (T)`
//! - Variadic inputs (and outputs!) `(...Integer) -> (Array<Integer>)`
//! - unions `"a" | "b" | "c"`
//! - intersections `{ a: Integer } & { b: String }`
//! - keyof `keyof { a: Integer }`
//! - index `{ a: Integer }["a"]`
//! - default values/types `<T>(T = Integer) -> (T)`
//! - Rank N polymorphism (*aka parameters can have their own generics*) `(<T>(T) -> (T)) -> ()`
//! - Custom inputs/outputs `Any -> Never`, `YourTypeA -> YourTypeB`
//!
//! **Features beyond typescript:**
//! - User defined type operations `<A, B>(A, B) -> (A * B)` (e.g. for SI units. You can define any type operation you like)
//! - Propagating tags - Nodes can have and require tags. Tags propagate through the graph. This enables enforcing something like const functions in rust (a node can only be const if itself is const and all its ingoing edges are too).
//!
//! **Not yet supported:**
//! - Mapped types `{ [K in keyof { a: Integer }]: K }`
//! - infer keyword `infer T` (although this would be easy to add if needed)
//!
//! ## Is this for me?
//! If you or your team is building a visual node editor in a place that can run rust code (wasm works great for web apps), then you should consider nodety.
//!
//! It looks like you're familiar with rust so I probably don't need to tell you about all the benefits a good type system has to offer.
//! With nodety you can build node editors that are just as type safe as your application code.
//!
//! **Using nodety can empower you to build the following features:**
//! - Autocompletion - Use nodety to generate a list of all valid nodes a user can connect. See the [autocompletion module](crate::autocomplete).
//! - Helpful error messages - Tell the user exactly what connections are invalid and why (live)
//! - User defined nodes - Enable users to define their own reusable nodes with input and output types
//! - Dynamic control inputs - All nodes need some sort of controls. You can use nodety to infer that
//!   input A of your add node has to be a integer input instead of a float input because the user connected
//!   an integer to B. Or anything you can dream of
//!
//! **Why this might not be for you:**
//!
//! To provide its features, nodety must impose one requirement on the node graph:
//!
//! All nodes **MUST** be representable by the [NodeSignature] struct. TLDR: when your nodes can be represented
//! by a function signature in any common programming language, you're good to go. To go in a bit more detail, the following is required:
//! * Input and output ids are numeric and continuous. (Ports are represented by a Vec, not a Map)
//! * Nodes can have variable length inputs and outputs (vargs), but only the last input/output can be variadic. As in `(Float, ...Float) -> (Float, ...Float)`.
//!   Something like `(...Float, Float) -> (...Float, Float)` is not possible.
//!
//! ## Let the code talk
//! Enough talking. Here is an example that demonstrates some of nodety's powers:
//!
//! ```rust
//! # use nodety::{
//! #     Nodety,
//! #     demo_type::DemoType,
//! #     inference::InferenceConfig,
//! #     type_expr::{node_signature::NodeSignature, TypeExpr},
//! #     scope::LocalParamID,
//! #     };
//! #  use std::str::FromStr;
//! /// Mapper Example:
//! /// Generic mapper node that infers the source type from an input
//! /// array and the output from a mapper node signature.
//! ///
//! /// here nodety infers that
//! /// T = Integer
//! /// U = String
//! ///                                            
//! /// |- Array source -----------|               |- <T, U>Map ----------------|
//! /// |           Array<Integer> | ------------> | Array<T>          Array<U> |
//! /// |--------------------------|               |                            |
//! ///                                    /-----> | (T) -> (U)                 |
//! /// |- Mapper -----------------|       |       |----------------------------|
//! /// |    (Integer) -> (String) | ------/
//! /// |--------------------------|
//! fn main() {
//!     // DemoType is a reference Type implementation
//!     let mut nodety = Nodety::<DemoType /* <-- Your own type implementation here */ >::new();
//!     
//!     // These signatures could be defined using normal rust code but that would be much less concise.
//!     let array_source_node = "() -> (Array<Integer>)".parse::<NodeSignature<_>>().unwrap();
//!     let mapper_node = "() -> ((Integer) -> (String))".parse::<NodeSignature<_>>().unwrap();
//!     let map_node = "<T, U>(Array<T>, (T) -> (U)) -> (Array<U>)".parse::<NodeSignature<_>>().unwrap();
//!
//!     // Add the nodes to the nodety graph.
//!     let array_source_node_id = nodety.add_node(array_source_node).unwrap();
//!     let mapper_node_id = nodety.add_node(mapper_node).unwrap();
//!     let map_node_id = nodety.add_node(map_node).unwrap();
//!
//!     // Add the edges between the nodes.
//!     nodety.add_edge(array_source_node_id, map_node_id, 0, 0);
//!     nodety.add_edge(mapper_node_id, map_node_id, 0, 1);
//!
//!     // Perform type inference.
//!     let inference = nodety.infer(InferenceConfig::default());
//!
//!     // Validate the graph using the inferred types.
//!     let errors = nodety.validate(&inference);
//!     // validate returns Vec<ValidationError> so that all errors can
//!     // be displayed to the user. ValidationError knows about where in
//!     // the graph an error occurred and contains detailed diagnostics.
//!     assert!(errors.is_empty());
//!
//!     let (inferred_t, _) = inference.get(&map_node_id).unwrap().lookup_inferred(&"T".into()).unwrap();
//!     assert_eq!(TypeExpr::from_str("Integer").unwrap(), inferred_t);
//!
//!     let (inferred_u, _) = inference.get(&map_node_id).unwrap().lookup_inferred(&"U".into()).unwrap();
//!     assert_eq!(TypeExpr::from_str("String").unwrap(), inferred_u);
//!
//!     // Success! T = Integer and U = String
//! }
//! ```
//! ## Features
//! This crate has the following Cargo features:
//!
//! - `parser`: Enables parsing type expressions and signatures from &str as you see above. (enabled by default)
//! - `serde`: Enables serialization and deserialization of types using serde.
//! - `json-schema`: Adds support for JSON Schema generation using Schemars. Useful for generating OpenAPI docs.
//! - `wasm`: Enables wasm support. This is useful for creating a type safe ts wrapper with wasm.
//! - `tsify`: Generates TypeScript types using Tsify. This enables creating a type safe ts wrapper with wasm.
//!
//! ## Minimum Supported Rust Version (MSRV)
//!
//! The MSRV is **1.85.0** (Rust edition 2024).
//!
//! ## Introduction
//! Read this chapter first to gain an overview of the modules nodety provides.
//!
//! ### Types
//! Firstly Nodety provides the [`Type`] trait, which you need to implement to represent your own domain specific types.
//!
//! The following is all you need to get started with your own types:
//! ```
//! # use nodety::Type;
//! #[derive(Clone, Debug, PartialEq)]
//! enum MyType {
//!   Integer,
//!   String,
//! }
//!
//! impl Type for MyType {
//!   type Operator = ();
//! }
//!
//! assert!(MyType::Integer.supertype_of(&MyType::Integer));
//! assert!(!MyType::String.supertype_of(&MyType::Integer));
//! ```
//!
//! The type trait has the method [`Type::supertype_of`] which is used to determine if a type is a supertype of another type.
//! The default implementation simply returns true if the types are equal, which is enough for simple cases.
//!
//! For a reference implementation have a look at the [`demo_type`] module.
//!
//! ### Type expressions
//! On top of that nodety provides the data structure [`TypeExpr`] which is generic over a Type `T`.
//! It is a recursive type that is used to represent all type in nodety. Unions, intersections, etc.
//! A TypeExpr can also represent a so called "NodeSignature" which is analog to a function signature in any programming language.
//!
//! If you've taken a look at the definition you might wonder what the second generic `S` parameter is for.
//! When you define type expressions it will probably be set to Unscoped which is en empty enum aka the empty `!` type. Internally this gets converted to
//! [ScopePortal](crate::type_expr::ScopePortal) which enables types to create scopes that reference outside their own scope.
//! This is needed to enable some of the internal logic but uses an Rc so is neither send nor sync. Furthermore there is no meaningful way to
//! serialize / deserialize it.
//!
//! Inside Nodety `S` can be one of these:
//! - Unscoped - The expression does not jump between scopes
//! - ScopePortal - The expression could jump between scopes (only used internally)
//! - ErasedScopePortal - an expression that used to be a `TypeExpr<T, ScopePortal<T>>` but all scope portals got erased.
//!   This is used for subtyping diagnostics to make the once scoped expression serializable.
//!
//! There are the following conversions:
//! - `Unscoped` -> `ScopePortal` (lossless)
//! - `ScopePortal` -> `ErasedScopePortal` (lossy)
//!
//! ### Nodety
//! Next up is [`Nodety`]. A data structure that contains a set of nodes and edges between them.
//! It has the [`infer`](crate::nodety::Nodety::infer) and [`validate`](crate::nodety::Nodety::validate) methods to perform type
//! inference and checking respectively. Its API aims to be as straightforward and easy to use as possible.
//!
//! ### Scopes
//! Nodety uses [Scope](crate::scope::Scope)s to manage visibilities of type parameters. Any node (that has one or more type parameters)
//! implicitly has a scope. The node's parameters are accessible only within that scope. Scopes can be nested and type expressions in inner scopes can access
//! everything from parent scopes.
//!
//! Type expressions should always be handled together with a scope. Because only with the correct scope will the type variables of the expressions be able to get resolved.
//!
//! **Scopes are additive** - What was once inferred can't get undefined without creating a new scope. The internals rely on this. This is to ensure that all types can always
//! only get more specific, but not less (The only exception being conditional types whose condition changed)
//!
//! *So far so good.* But when you add nodes you can also add them as children of some other node. When doing this, the child nodes will live inside the parent node's scope.
//! This is useful for the following scenarios:
//! - Your graphs have inputs and outputs and you want to infer what types the inputs and outputs are. You could naively use the generic types I1, ...IN for your inputs
//!   and O1, ...ON for your outputs, then run inference. This will work, however, say the user connects one of the inputs directly to an output, then the output will be inferred
//!   to the input. But you don't necessarily know what scope the inferred type is in.
//!   If you add one "root" node to your graph and add all nodes as children to that root node, you can define your I1, ...IN and O1, ...ON parameters in the root nodes
//!   scope and only reference them in your inputs and outputs. This way you can easily infer the types of your inputs and outputs. and even infer generic parameters.
//! - Secondly this comes in handy if you allow your user to actually nest nodes in some way or another. Examples might be blenders simulation zone, or a loop node.
//!
//! ### Tags
//! Tags are an optional mechanism for propagating non-type metadata through
//! the graph. Nodes can provide and require tags. Tags propagate forward
//! through the graph and are validated on incoming edges.
//! See [`NodeSignature::tags`](crate::type_expr::node_signature::NodeSignature::tags)
//! and [`NodeSignature::required_tags`](crate::type_expr::node_signature::NodeSignature::required_tags)
//! for details.
//!
//! ### Notation
//! Defining node signatures using rust code is quite verbose. For this reason nodety provides a compact text notation for defining node signatures and type expressions.
//! The syntax is inspired by a mix of rust and typescript. Have a look at the [notation module](crate::notation) for more details.
//!
//! **Note:** To enable parsing of types, the `parser` feature must be enabled (enabled by default).
//!
//! After you implement [ParsableType](crate::notation::parse::ParsableType) for your own types, you will be able to use the [FromStr](std::str::FromStr) trait
//! to parse type expressions with your own types.
//!
//! Formatting types tries to follow the same syntax as parsing. After you implement [FormattableType](crate::notation::format::FormattableType) for your own types,
//! you will also be able to use the [Display](std::fmt::Display) trait for type expressions containing your types.
//! ```
//! # use nodety::{NodeSignature, TypeExpr};
//! # use nodety::demo_type::DemoType;
//! # use std::str::FromStr;
//! let node_signature = "<T>(T) -> (T)".parse::<NodeSignature<DemoType>>().unwrap();
//! assert_eq!("<T>(T) -> (T)", format!("{}", node_signature));
//!
//! let expr = "Integer".parse::<TypeExpr<DemoType>>().unwrap();
//! assert_eq!("Integer", format!("{}", expr));
//! ```
//!
//! ## State of this crate
//!
//! This crate is still in its early stages. The core architecture and features have been worked out. But the API might still change in the near future on the
//! path to finding the most ergonomic abstractions.
//!
pub mod autocomplete;
pub mod demo_type;
pub mod node_sorting;
pub mod nodety;
pub mod nodety_cached;
pub use nodety::{Node, Nodety, inference, validation};
pub use nodety_cached::NodetyCached;
pub mod notation;
pub mod scope;
mod r#type;
pub use r#type::{NoOperator, Type};
pub mod type_expr;
pub use type_expr::{TypeExpr, node_signature::NodeSignature};

#[cfg(feature = "proptest")]
pub mod arbitrary;
