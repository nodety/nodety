//! Text notation for type expressions and node signatures.
//!
//! Nodety provides a compact text notation for defining [`NodeSignature`](crate::type_expr::node_signature::NodeSignature)s
//! and [`TypeExpr`](crate::type_expr::TypeExpr)s. Use [`ParsableType`](parse::ParsableType) and [`FormattableType`](format::FormattableType)
//! to extend parsing and formatting for custom types.
//!
//! # Notation Structure
//!
//! ## Node Signatures
//!
//! A node signature has the form `[<params>] (inputs) -> (outputs) [effects <A> <B>] [handles <C>]`:
//!
//! ```text
//! <T, U extends Comparable>(T, U) -> (Integer)
//! <T>(Integer = 0) -> (T)           // default input type
//! () -> (Never)                     // no params, empty inputs, Never output
//! () -> (Integer) effects <A> <B>     // effects A and B
//! <E>() -> () effects <E> handles <A>  // handles the effect A of its children
//! ```
//!
//! - **Parameters** (optional): `<T>`, `<T, U extends Bound>` or `<T = Any>`
//! - **Inputs**: `(port1, port2, ...)` or `(port1, port2, ...rest)` for variadic
//! - **Outputs**: same format
//! - **Defaults**: `(Type = default)` on an input gives a default when unconnected
//! - **Effects** (optional): `effects <A> <B>`, one or more type expressions in angle brackets, which is the union
//!   `A | B` (`effects <A | B>` is the same). Without them the effects are `Never`.
//! - **Handled effects** (optional): `handles <A> <B>`, same form, after the effects.
//!
//! The brackets end the effects, so in `(T) -> (U) effects <E> | Integer` the union is between the signature and
//! `Integer`.
//! Whitespace around `effects`, `handles` and the brackets is optional: `() -> ()effects<A><B>handles<C>`. Outputs
//! written as a bare type parameter need a space before the keyword: `() -> T effects <A>`.
//!
//! ## Type Expressions
//!
//! Parsing tries alternatives in order. Precedence (loosest to tightest):
//!
//! | Precedence | Syntax | Example |
//! |------------|--------|---------|
//! | 0 | Bounded | `A..B`, `=A` |
//! | 1 | Conditional | `A extends B ? C : D` |
//! | 2 | Union | `A \| B \| C` |
//! | 3 | Intersection | `A & B & C` |
//! | 4 | Index | `A[B]` |
//! | 5 | Operation | `A * B`, `A / B` (custom operators) |
//! | 6 | Atomic | see below |
//!
//! ## Bounded Types
//!
//! `L..U` is a type `X` with `L ⊆ X ⊆ U`, `=X` is short for `X..X`. Used for record fields in parameter bounds that a
//! node writes `L` into and reads `U` from: `<C extends {score: Integer..Integer | Unit}>(Table<C>) -> (Table<C>)`.
//! Bounds can't be bounded themselves without parentheses: `A | B..C | D` is `(A | B)..(C | D)`.
//! `..` does not clash with variadic ports (`...Rest`) because those start a port, while `..` follows a type.
//!
//! ## Atomic Type Expressions
//!
//! - **Node signature**: `(inputs) -> (outputs)` or `<T>(T) -> (T)`
//! - **Built-in**: `Any`, `Never`
//! - **keyof**: `keyof T` — keys of a record/object type
//! - **Mapped type**: `{[K in keyof T]: V}` — the fields of `T`, each with the type `V` where `K` is the field's key
//! - **Type parameter**: `T`, `U`, or `#0`, `#1` (numeric ids). Prefix `!` to exclude from inference: `!T`
//! - **Parentheses**: `(type_expr)` for grouping
//! - **Custom types**: Parsed via [`ParsableType::parse`](parse::ParsableType::parse) (e.g. `Integer`, `Array<T>`, `{a: T}`)
//!
//! ## Port Types (Input/Output Lists)
//!
//! ```text
//! ()                    // empty
//! (A, B, C)             // fixed ports
//! (A, B, ...Rest)       // variadic: Rest applies to extra args
//! (...T)                // only variadic
//! (name: Type)          // optional label (ignored for indexing)
//! (Type = default)      // default when unconnected
//! ```
//!
//! ## Type Parameters
//!
//! ```text
//! <T>                           // unconstrained
//! <T extends Comparable>        // upper bound
//! <T = Integer>                 // default when not inferred
//! <T extends Comparable = Unit>  // bound and default
//! <#0, #1>                      // numeric ids (for non-identifier params)
//! <!T>                          // This instance of T will not be used for inference
//! <rigid N extends Integer>     // never inferred, see TypeParameter::rigid
//! ```
//!
//! ## Demo Type System (Reference)
//!
//! The [`DemoType`](crate::demo_type::DemoType) implementation supports:
//!
//! | Notation | Type |
//! |----------|------|
//! | `Integer`, `Float`, `Boolean`, `String`, `Unit` | Primitives |
//! | `Comparable`, `Countable`, `Sortable` | Interfaces |
//! | `"literal"` or `'literal'` | String literal |
//! | `Array<T>` | Generic array |
//! | `{a: T, b: U}` | Record with fields |
//! | `SI(scale, s, m, kg, a, k, mol, cd)` | SI units |
//! | `AnySI` | Any SI unit |
//! | `A * B`, `A / B` | SI multiplication/division |
//! | `3`, `-1` | Integer literals, subtypes of `Integer` |
//! | `A + B`, `A - B` | Integer addition/subtraction, linear over rigid parameters |
//! | `Nondeterministic`, `Fallible` | Effects |
//!
//! ## Identifiers and Strings
//!
//! - **Identifiers**: `[a-zA-Z_][a-zA-Z0-9_-]*` (alphanumeric, underscore, hyphen)
//! - **Quoted strings**: `"..."` or `'...'` with escapes: `\"`, `\\`, `\n`, `\r`, `\t`
//! - **Record field names**: identifier or quoted string
//!
//! ## Type Hints
//!
//! [`TypeHints`](crate::nodety::node::TypeHints) implements `FromStr` for parsing `T = Integer, U = String` into a map of parameter-to-type for pre-inference hints.

pub mod format;
#[cfg(feature = "parser")]
pub mod parse;
