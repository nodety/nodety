//! The reference kinds a [TypeExpr] can contain.
//!
//! Every [TypeExpr] carries a second generic parameter `R: TypeRef` that decides which kind of
//! *references to the outside* the expression is allowed to contain. This lets the type system
//! prove at compile time that an expression is fully self contained, or allow it to reference
//! type parameters, without every consumer having to defensively handle both cases.
use crate::{
    Type, TypeExpr,
    scope::{LocalParamID, ScopePointer},
    type_expr::ScopedTypeExpr,
};
use std::fmt::Debug;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

mod private {
    pub trait Sealed {}
}

/// Decides what a [TypeExpr] is allowed to reference. Sealed — nodety ships all implementors:
///
/// - [NoRef] — the expression is fully self contained. It can not reference anything outside of
///   itself, which makes it understandable without any context.
/// - [ParamRef] — the expression may reference type parameters, but never jumps between scopes.
///   This is what you get when parsing `<T>(T) -> (T)`.
/// - [ScopedTypeRef] — the expression may reference type parameters *and* may carry sub
///   expressions that live in a foreign scope. Only produced internally.
pub trait TypeRef: private::Sealed + Clone + Debug + PartialEq {
    /// If this reference refers to a type parameter, returns it.
    fn as_param_ref(&self) -> Option<&ParamRef>;
}

/// A [TypeRef] that is able to represent a reference to a type parameter.
///
/// Implemented by every [TypeRef] except [NoRef]. Functions that produce type parameters (the
/// parser, the proptest strategies, ...) are bound on this, so `TypeExpr<T, NoRef>` can not be
/// built with a parameter reference in it.
pub trait ParamTypeRef: TypeRef {
    fn from_param(param: ParamRef) -> Self;
}

/// A [TypeRef] that can be resolved when a [ScopePointer] is at hand, letting functions work with
/// [NoRef], [ParamRef] and [ScopedTypeRef] expressions alike.
pub trait AsScopedRef<T: Type>: TypeRef {
    fn view(&self) -> ScopedRefView<'_, T>;

    fn into_scoped_ref(self) -> ScopedTypeRef<T>;
}

/// Borrowed view on any [AsScopedRef]. See [AsScopedRef::view].
pub enum ScopedRefView<'a, T: Type> {
    Param(&'a ParamRef),
    /// An expression that lives in a foreign scope.
    ScopedExpr {
        expr: &'a ScopedTypeExpr<T>,
        scope: &'a ScopePointer<T>,
    },
}

/// crate local version of [std::convert::Infallible].
///
/// A `TypeExpr<T, NoRef>` provably contains neither type parameters nor scope portals, so it can
/// be understood without a [Scope](crate::scope::Scope).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(type = "never"))]
pub enum NoRef {
    // Never add a variant here!
}

/// References a local type parameter. This is context sensitive because parameters are scoped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct ParamRef {
    pub param_id: LocalParamID,
    /// If false, this reference will not be used to collect candidates for the parameter.
    /// In the notation this is written as `!T`.
    pub infer: bool,
}

impl ParamRef {
    pub fn new(param_id: impl Into<LocalParamID>, infer: bool) -> Self {
        Self { param_id: param_id.into(), infer }
    }

    /// A reference that participates in candidate collection.
    pub fn inferring(param_id: impl Into<LocalParamID>) -> Self {
        Self::new(param_id, true)
    }
}

/// A reference of an expression that knows about scopes.
///
/// Unifies "references a type parameter" and "this sub expression is to be read in a different
/// scope" — the two things that make a type expression context sensitive.
#[derive(Debug, Clone, PartialEq)]
pub enum ScopedTypeRef<T: Type> {
    Param(ParamRef),
    /// Represents an expression inside a foreign scope.
    ScopedExpr {
        expr: Box<ScopedTypeExpr<T>>,
        scope: ScopePointer<T>,
    },
}

impl<T: Type> ScopedTypeRef<T> {
    pub fn scoped_expr(expr: ScopedTypeExpr<T>, scope: ScopePointer<T>) -> Self {
        Self::ScopedExpr { expr: Box::new(expr), scope }
    }
}

/// A scope portal that used to be there but got removed in order to be serializable
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "type",
        content = "data",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub enum ErasedScopedTypeRef<T: Type> {
    Param(ParamRef),
    ScopedExpr { expr: Box<TypeExpr<T, ErasedScopedTypeRef<T>>> },
}

impl<T: Type> ErasedScopedTypeRef<T> {
    /// Strips [ScopePointer] data from scope portals while preserving nested expression structure.
    ///
    /// Used when serializing supertype diagnostics: scope portals become
    /// [ErasedScopedTypeRef::ScopedExpr] nodes without a scope field.
    pub fn from_as_scoped<R: AsScopedRef<T>>(r: R) -> Self {
        match r.view() {
            ScopedRefView::Param(param) => Self::Param(*param),
            ScopedRefView::ScopedExpr { expr, .. } => {
                Self::ScopedExpr { expr: Box::new(expr.clone().map_refs(ErasedScopedTypeRef::from_as_scoped)) }
            }
        }
    }
}

impl private::Sealed for NoRef {}
impl private::Sealed for ParamRef {}
impl<T: Type> private::Sealed for ScopedTypeRef<T> {}
impl<T: Type> private::Sealed for ErasedScopedTypeRef<T> {}

impl TypeRef for NoRef {
    fn as_param_ref(&self) -> Option<&ParamRef> {
        match *self {
            // Never
        }
    }
}

impl<T: Type> TypeRef for ErasedScopedTypeRef<T> {
    fn as_param_ref(&self) -> Option<&ParamRef> {
        None
    }
}

impl TypeRef for ParamRef {
    fn as_param_ref(&self) -> Option<&ParamRef> {
        Some(self)
    }
}

impl<T: Type> TypeRef for ScopedTypeRef<T> {
    fn as_param_ref(&self) -> Option<&ParamRef> {
        match self {
            Self::Param(param) => Some(param),
            Self::ScopedExpr { .. } => None,
        }
    }
}

impl ParamTypeRef for ParamRef {
    fn from_param(param: ParamRef) -> Self {
        param
    }
}

impl<T: Type> ParamTypeRef for ScopedTypeRef<T> {
    fn from_param(param: ParamRef) -> Self {
        Self::Param(param)
    }
}

impl<T: Type> AsScopedRef<T> for NoRef {
    fn view(&self) -> ScopedRefView<'_, T> {
        match *self {
            // Never
        }
    }

    fn into_scoped_ref(self) -> ScopedTypeRef<T> {
        match self {
            // Never
        }
    }
}

impl<T: Type> AsScopedRef<T> for ParamRef {
    fn view(&self) -> ScopedRefView<'_, T> {
        ScopedRefView::Param(self)
    }

    fn into_scoped_ref(self) -> ScopedTypeRef<T> {
        ScopedTypeRef::Param(self)
    }
}

impl<T: Type> AsScopedRef<T> for ScopedTypeRef<T> {
    fn view(&self) -> ScopedRefView<'_, T> {
        match self {
            Self::Param(param) => ScopedRefView::Param(param),
            Self::ScopedExpr { expr, scope } => ScopedRefView::ScopedExpr { expr, scope },
        }
    }

    fn into_scoped_ref(self) -> ScopedTypeRef<T> {
        self
    }
}

/// A [TypeRef] that never jumps between scopes: every reference it can hold is a plain reference
/// to a type parameter. Implemented by [NoRef] and [ParamRef], but not by [ScopedTypeRef].
///
/// This is what makes an expression writable in the notation — a scope portal has no syntax. Use
/// [try_into_unscoped](crate::TypeExpr::try_into_unscoped) or
/// [force_remove_scope_portals](crate::TypeExpr::force_remove_scope_portals) to get there from a
/// scoped expression.
pub trait UnscopedRef: TypeRef {
    /// Unlike [TypeRef::as_param_ref] this is total: there is nothing else a reference of this
    /// kind could be.
    fn as_param(&self) -> &ParamRef;
}

impl UnscopedRef for NoRef {
    fn as_param(&self) -> &ParamRef {
        match *self {
            // Never
        }
    }
}

impl UnscopedRef for ParamRef {
    fn as_param(&self) -> &ParamRef {
        self
    }
}
