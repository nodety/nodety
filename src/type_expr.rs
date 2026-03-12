use crate::{
    inference::infer,
    nodety::inference::{Flow, InferenceConfig, InferenceDirection, InferenceStep},
    scope::{GlobalParameterId, LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{
        conditional::Conditional,
        node_signature::{
            CyclicReferenceError, NodeSignature, port_types::PortTypes, type_parameters::TypeParameters,
            validate_type_parameters,
        },
        subtyping::SupertypeResult,
    },
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    fmt::Debug,
};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

pub mod candidate_collection;
pub mod conditional;
pub mod conversions;
pub mod index;
pub mod intersections;
pub mod keyof;
pub mod node_signature;
pub mod normalization;
pub mod subtyping;
pub mod traversal;

pub use conversions::HasScopePortals;

mod private {
    pub trait Sealed {}
}

pub trait TypeExprScope: private::Sealed + Clone {}

#[derive(Debug, Clone)]
pub struct ScopePortal<T: Type> {
    portal: ScopePointer<T>,
}

impl<T: Type> ScopePortal<T> {
    pub fn new(portal: ScopePointer<T>) -> Self {
        Self { portal }
    }
}

impl<T: Type> PartialEq for ScopePortal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.portal == other.portal
    }
}

/// In the future this could contain information about the portal scope.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone, PartialEq)]
pub struct ErasedScopePortal;

/// crate local version of [std::convert::Infallible]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unscoped {
    // Never add a variant here!
}

impl private::Sealed for Unscoped {}
impl private::Sealed for ErasedScopePortal {}
impl<T: Type> private::Sealed for ScopePortal<T> {}

impl<T: Type> TypeExprScope for ScopePortal<T> {}
impl TypeExprScope for Unscoped {}
impl TypeExprScope for ErasedScopePortal {}

/// Type expression—the core type representation in nodety.
///
/// Can represent unions, intersections, conditional types, type variables, keyof, index access,
/// node signatures (function-like types), port types, and more. Generic over [`Type`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        rename_all_fields = "camelCase",
        tag = "type",
        content = "data",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize, S: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, S: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, S: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Default)]
pub enum TypeExpr<T: Type, S: TypeExprScope = Unscoped> {
    // An atomic "leaf" type.
    Type(T),

    /// Constructors can represent types like `Map<K, V>` and records. The
    /// intersection of two constructors A and B has all parameters of A and B.
    /// Those present in both A and B will be the intersection of both.
    ///
    /// See also the [Type] trait documentation for more information on constructors.
    Constructor {
        inner: T,
        parameters: BTreeMap<String, TypeExpr<T, S>>,
    },

    /// Custom defined operator.
    ///
    /// See also the [Type] trait documentation for more details about operators.
    /// And the [SIUnit](crate::demo_type::SIUnit) type for a reference implementation with operators.
    Operation {
        a: Box<TypeExpr<T, S>>,
        operator: T::Operator,
        b: Box<TypeExpr<T, S>>,
    },

    /// References a local type parameter. This is context sensitive because parameters are scoped.
    ///
    /// (parameter_id, infer)
    ///
    /// if infer is false, this expression will not be used to collect candidates for the parameter.
    /// In the notation this is written as `!T`.
    /// @todo: refactor this into `{ id: LocalParamID, infer: bool }`
    TypeParameter(LocalParamID, bool),

    NodeSignature(Box<NodeSignature<T, S>>),

    /// A parameter list (a, b, ...c)
    PortTypes(Box<PortTypes<T, S>>),

    /// A type that is either left or right
    Union(Box<TypeExpr<T, S>>, Box<TypeExpr<T, S>>),

    /// Describes the key type of the expression.
    KeyOf(Box<TypeExpr<T, S>>),

    /// Determines the type that `expr` returns when indexed with the type `index`.
    Index {
        expr: Box<TypeExpr<T, S>>,
        index: Box<TypeExpr<T, S>>,
    },

    /// A type that is both A & B.
    Intersection(Box<TypeExpr<T, S>>, Box<TypeExpr<T, S>>),

    /// Represents the following: `t_test` extends `t_test_bound` ? `t_then` : `t_else`
    /// Besides the infer keyword, works almost exactly like conditional types in typescript.
    /// Checkout [this doc](https://www.typescriptlang.org/docs/handbook/2/conditional-types.html) for a good guide on the ts conditionals.
    Conditional(Box<Conditional<T, S>>),

    /// The universal supertype encompassing all types — and the bane of every linter.
    #[default]
    Any,

    /// The Never type. Also known as the bottom type.
    /// All types are supertypes of never.
    Never,

    // Scope can only be infallible or a ScopePointer<T>.
    // If it is infallible it will never be constructed and thus never needs to get serialized.
    // If it is ScopePointer<T> the Serialize bound doesn't hold and serde won't be derived for Self anyway.
    #[cfg_attr(feature = "serde", serde(skip))]
    ScopePortal {
        expr: Box<TypeExpr<T, S>>,
        scope: S,
    },
}

pub type ScopedTypeExpr<T> = TypeExpr<T, ScopePortal<T>>;

pub type UnscopedTypeExpr<T> = TypeExpr<T, Unscoped>;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum TypeExprValidationError {
    UnknownVar(LocalParamID),
    CyclicReference,
}

impl<T: Type> TypeExpr<T, ScopePortal<T>> {
    pub fn union_with(self, other: Self) -> Self {
        TypeExpr::Union(Box::new(self), Box::new(other))
    }

    pub fn intersection_with(self, other: Self) -> Self {
        TypeExpr::Intersection(Box::new(self), Box::new(other))
    }

    pub fn validate(&self, scope: &ScopePointer<T>) -> Result<(), TypeExprValidationError> {
        let mut res = Ok(());
        self.traverse(
            scope,
            &mut |expr, scope, _is_tl_union| {
                if let TypeExpr::TypeParameter(param, _) = expr {
                    if scope.lookup(param).is_none() {
                        res = Err(TypeExprValidationError::UnknownVar(*param));
                    }
                }
                if let (_, Some(own_params)) = self.extract_generic_parameters() {
                    if let Err(CyclicReferenceError) = validate_type_parameters(own_params) {
                        res = Err(TypeExprValidationError::CyclicReference);
                    }
                }
            },
            true,
        );

        res
    }

    /// Collects all referenced type parameters that reference a parameter outside of self.
    /// Params that reference variables defined somewhere inside self are not included.
    pub fn collect_references_type_params(&self) -> HashSet<LocalParamID> {
        let mut params = HashSet::new();

        self.traverse(
            &ScopePointer::new_root(),
            &mut |expr, scope, _is_tl_union| {
                let TypeExpr::TypeParameter(param, _infer) = expr else {
                    return;
                };
                if scope.lookup(param).is_none() {
                    params.insert(*param);
                }
            },
            false,
        );

        params
    }

    pub fn contains_type_param(&self) -> bool {
        let mut contains_generic = false;
        let dummy_scope = ScopePointer::new_root();
        self.traverse(
            &dummy_scope,
            &mut |expr, _dummy_scope, _is_tl_union| {
                contains_generic |= matches!(expr, TypeExpr::TypeParameter(_, _));
            },
            false,
        );

        contains_generic
    }

    /// Tests if the expression references a type parameter that points outside of the expression itself.
    /// # Examples
    /// `<T>(T) -> ()` => returns false because T is defined within the expression
    /// `T & {}` => returns true because T is defined outside of the expression
    pub fn references_external_type_param(&self) -> bool {
        let mut contains_generic = false;
        let dummy_scope = ScopePointer::new_root();
        self.traverse(
            &dummy_scope,
            &mut |expr, scope, _is_tl_union| {
                let TypeExpr::TypeParameter(param, _infer) = expr else {
                    return;
                };
                if scope.lookup(param).is_none() {
                    contains_generic = true;
                } else {
                    // If look up succeeds, the parameter must have been defined within self.
                }
            },
            false,
        );

        contains_generic
    }

    pub fn contains_specific_type_param(&self, needle: &LocalParamID) -> bool {
        let mut contains = false;
        let dummy_scope = ScopePointer::new_root();
        self.traverse(
            &dummy_scope,
            &mut |expr, _dummy_scope, _is_tl_union| {
                let TypeExpr::TypeParameter(expr_param, _infer) = expr else {
                    return;
                };
                if expr_param == needle {
                    contains = true;
                }
            },
            false,
        );

        contains
    }

    pub fn contains_uninferred(&self, scope: &ScopePointer<T>) -> bool {
        let mut contains_uninferred = false;
        self.traverse(
            scope,
            &mut |expr, traverse_scope, _is_tl_union| {
                let TypeExpr::TypeParameter(param, _infer) = expr else {
                    return;
                };
                contains_uninferred |= traverse_scope.lookup_inferred(param).is_none();
            },
            false,
        );
        contains_uninferred
    }

    /// Tests if the expression references the needle anywhere in its structure.
    /// # Parameters
    /// - `scope`: The scope of self
    /// - `needle`: The parameter to look for
    pub fn references(&self, needle: &HashSet<GlobalParameterId<T>>, scope: &ScopePointer<T>) -> bool {
        // let Some(initial_depth) = state.locate_parameter(parameter, ctx) else { return false };
        let mut contains = false;

        self.traverse(
            scope,
            &mut |expr, scope, _is_tl_union| {
                let TypeExpr::TypeParameter(param, _infer) = expr else {
                    return;
                };
                let Some(var_scope) = scope.lookup_scope(param) else {
                    return;
                };
                let global_id = GlobalParameterId { scope: var_scope, local_id: *param };
                if needle.contains(&global_id) {
                    contains = true;
                }
            },
            false,
        );
        contains
    }

    /// Returns the parameters of the TypeExpr if it has any.
    /// Currently only NodeSignatures have type parameters
    #[allow(clippy::type_complexity)]
    pub fn extract_generic_parameters<'a>(
        &'a self,
    ) -> (Cow<'a, Self>, Option<&'a BTreeMap<LocalParamID, TypeParameter<T, ScopePortal<T>>>>) {
        match self {
            Self::NodeSignature(sig) if !sig.parameters.is_empty() => (
                Cow::Owned(TypeExpr::NodeSignature(Box::new(NodeSignature {
                    parameters: TypeParameters::default(),
                    ..*sig.clone()
                }))),
                Some(&*sig.parameters),
            ),
            _ => (Cow::Borrowed(self), None),
        }
    }

    /// Most type expressions can't widen. They only get narrower when parameters get inferred.
    ///
    /// The only exception to this are conditional types. When the test or test bound gets inferred, that
    /// might lead to the other branch to take effect which could widen the type.
    ///
    /// # Returns
    /// true if `self` contains a conditional whose t_test or t_test_bound references uninferred parameters.
    pub fn could_widen(&self, scope: &ScopePointer<T>) -> bool {
        let mut could_widen = false;
        self.traverse(
            scope,
            &mut |type_expr, scope, _is_top_level_union| {
                let TypeExpr::Conditional(conditional) = type_expr else {
                    return;
                };
                if conditional.t_test.contains_uninferred(scope) || conditional.t_test_bound.contains_uninferred(scope)
                {
                    could_widen = true;
                }
            },
            true,
        );
        could_widen
    }

    /// # Returns
    /// `true` if `self` is and will always be never.
    /// `false` if `self` is either not never but could turn into never when more params get inferred.
    pub fn is_never_forever(&self, scope: &ScopePointer<T>) -> bool {
        self.is_never(scope).unwrap_or(false)
    }

    /// # Returns
    /// `None` if self references an uninferred type parameter that prevents detecting never.
    /// `Some(true)` if self is and wil always be never.
    /// `Some(false)` if self is and will never be never.
    pub fn is_never(&self, scope: &ScopePointer<T>) -> Option<bool> {
        match self {
            Self::Type(_) => Some(false),
            Self::Union(a, b) => Some(a.is_never(scope)? && b.is_never(scope)?),
            Self::Intersection(a, b) => {
                if a.is_never(scope).unwrap_or(false) || b.is_never(scope).unwrap_or(false) {
                    return Some(true);
                }
                let (intersection, scope) = Self::intersection(a, b, scope, scope)?;
                intersection.is_never(&scope)
            }
            Self::Operation { a, b, operator } => {
                let a = a.normalize(scope);
                let b = b.normalize(scope);
                T::operation(&a, operator, &b).is_never(scope)
            }
            Self::NodeSignature(_) => Some(false),
            Self::PortTypes(_) => Some(false),
            Self::Constructor { .. } => Some(false),
            Self::KeyOf(expr) => {
                let (key, key_scope) = expr.keyof(scope)?;
                key.is_never(&key_scope)
            }
            Self::Conditional(conditional) => conditional.distribute(scope)?.is_never(scope),
            Self::TypeParameter(param, _infer) => {
                let (registered, param_scope) = scope.lookup(param)?;
                if let Some((inferred, inferred_scope)) = registered.inferred() {
                    return inferred.is_never(&inferred_scope);
                }
                let (boundary, boundary_scope) = registered.get_boundary(param_scope);
                if boundary.is_never(&boundary_scope).unwrap_or(false) {
                    return Some(true);
                }
                None
            }
            // is_never(scope) is okay here because the result of index() must be in the same scope as expr.
            Self::Index { expr, index } => {
                let (index_type, index_scope) = expr.index(index, scope, scope)?;
                index_type.is_never(&index_scope)
            }

            Self::ScopePortal { expr, scope } => expr.is_never(&scope.portal),

            Self::Any => Some(false),
            Self::Never => Some(true),
        }
    }

    /// # Returns
    /// `true` if `self` is and will always be any.
    /// `false` if `self` is either not any but could turn into any when more params get inferred.
    pub fn is_any_forever(&self, scope: &ScopePointer<T>) -> bool {
        self.is_any(scope).unwrap_or(false)
    }

    /// # Returns
    /// `None` if self references an uninferred type parameter that prevents detecting any.
    /// `Some(true)` if self is and will always be any.
    /// `Some(false)` if self is not and will never be any.
    pub fn is_any(&self, scope: &ScopePointer<T>) -> Option<bool> {
        match self {
            Self::Type(_) => Some(false),
            Self::Union(a, b) => Some(a.is_any(scope)? || b.is_any(scope)?),
            Self::Intersection(a, b) => {
                if a.is_any(scope).unwrap_or(false) && b.is_any(scope).unwrap_or(false) {
                    return Some(true);
                }
                let (intersection, scope) = Self::intersection(a, b, scope, scope)?;
                intersection.is_any(&scope)
            }
            Self::Operation { a, b, operator } => {
                let a = a.normalize(scope);
                let b = b.normalize(scope);
                T::operation(&a, operator, &b).is_any(scope)
            }
            Self::NodeSignature(_) => Some(false),
            Self::PortTypes(_) => Some(false),
            Self::Constructor { .. } => Some(false),
            Self::KeyOf(expr) => {
                let (key, key_scope) = expr.keyof(scope)?;
                key.is_any(&key_scope)
            }

            Self::Conditional(conditional) => conditional.distribute(scope)?.is_any(scope),

            Self::TypeParameter(param, _infer) => {
                let (inferred, inferred_scope) = scope.lookup_inferred(param)?;
                inferred.is_any(&inferred_scope)
            }

            Self::ScopePortal { expr, scope } => expr.is_any(&scope.portal),

            // is_never(scope) is okay here because the result
            // of index() must be in the same scope as expr.
            Self::Index { expr, index } => {
                let (index_type, index_scope) = expr.index(index, scope, scope)?;
                index_type.is_any(&index_scope)
            }
            Self::Any => Some(true),
            Self::Never => Some(false),
        }
    }

    /// Builds an uninferred child scope for `self` with all parameters of `self` if it has any.
    /// # Returns
    /// - `self` without parameters. Removing the parameters is necessary when performing
    ///   inference because if they don't get removed here, they could get inferred again later.
    /// - the uninferred scope for `self`
    pub fn build_uninferred_child_scope<'a>(&'a self, scope: &ScopePointer<T>) -> (Cow<'a, Self>, ScopePointer<T>) {
        let (without_params, params) = self.extract_generic_parameters();
        let Some(params) = params else {
            return (without_params, ScopePointer::clone(scope));
        };
        let mut child_scope = Scope::new_child(scope);
        for (ident, param) in params {
            child_scope.define(*ident, param.clone());
        }
        (without_params, ScopePointer::new(child_scope))
    }

    /// If `self` has parameters, they will be attempted to be inferred from `source`.
    /// # Returns
    /// - `self` without parameters. Removing the parameters is necessary when performing
    ///   inference because if they don't get removed here, they could get inferred again later.
    /// - the inferred scope for `self`
    pub fn build_inferred_child_scope<'a>(
        &'a self,
        source: &Self,
        own_scope: &ScopePointer<T>,
        source_scope: &ScopePointer<T>,
    ) -> (Cow<'a, Self>, ScopePointer<T>) {
        let (self_without_params, own_params) = self.extract_generic_parameters();
        let Some(own_params) = own_params else {
            return (self_without_params, ScopePointer::clone(own_scope));
        };

        let mut to_infer = HashSet::new();

        let mut own_scope = Scope::new_child(own_scope);
        for (ident, param) in own_params {
            own_scope.define(*ident, param.clone());
        }
        let own_scope = ScopePointer::new(own_scope);
        for ident in own_params.keys() {
            to_infer.insert(GlobalParameterId { scope: ScopePointer::clone(&own_scope), local_id: *ident });
        }

        let flows = vec![Flow {
            source: source.clone(),
            target: self_without_params.as_ref().clone(),
            source_scope: ScopePointer::clone(source_scope),
            target_scope: ScopePointer::clone(&own_scope),
        }];
        let config = InferenceConfig {
            restrictions: Some(to_infer),
            steps: vec![
                // Infer candidates must be false here because if it is not than type
                // parameters in source might get inferred to type parameters in self
                // during candidate collection. When that happens the cycle detection
                // will prevent the variable in own_scope from ever getting inferred.
                InferenceStep {
                    direction: InferenceDirection::Forward,
                    allow_uninferred: false,
                    infer_candidates: false,
                    // These have to get ignored here
                    ignore_excluded: true,
                },
                InferenceStep {
                    direction: InferenceDirection::Forward,
                    allow_uninferred: true,
                    infer_candidates: false,
                    ignore_excluded: true,
                },
            ],
            ..Default::default()
        };
        infer(flows, &config);
        (self_without_params, own_scope)
    }

    /// If the root expression does not have any parameters, returns a borrowed
    /// reference to self. Otherwise, returns an owned copy of self with all
    /// parameters removed (only one level).
    pub fn without_params<'a>(&'a self) -> Cow<'a, Self> {
        match self {
            Self::NodeSignature(sig) if !sig.parameters.is_empty() => {
                Cow::Owned(Self::NodeSignature(Box::new(NodeSignature {
                    parameters: TypeParameters::default(),
                    ..*sig.clone()
                })))
            }
            expr => Cow::Borrowed(expr),
        }
    }

    /// # Returns
    /// - `Some(Cow::Borrowed(port_types))` if `self` is already a `PortTypes` expression.
    /// - `Some(Cow::Owned(port_types))` if `self` is not a `PortTypes` expression but can be normalized to one.
    /// - `None` if `self` is not a `PortTypes` expression and cannot be normalized to one.
    pub fn get_port_types(&self, scope: &ScopePointer<T>) -> Option<Cow<'_, PortTypes<T, ScopePortal<T>>>> {
        match self {
            Self::PortTypes(port_types) => Some(Cow::Borrowed(port_types.as_ref())),
            other => match other.normalize(scope) {
                Self::PortTypes(port_types) => Some(Cow::Owned(port_types.as_ref().clone())),
                _ => None,
            },
        }
    }
}

impl<T: Type, S: TypeExprScope> TypeExpr<T, S> {
    /// Creates a union type from at least one type.
    /// If only one type is given, returns that type.
    pub fn from_unions(first: Self, following: Vec<Self>) -> Self {
        let mut current = first;
        for exp in following {
            current = TypeExpr::Union(Box::new(current), Box::new(exp));
        }
        current
    }

    /// Creates an intersection type from at least one type.
    /// If only one type is given, returns that type.
    pub fn from_intersections(first: Self, following: Vec<Self>) -> Self {
        let mut current = first;
        for exp in following {
            current = TypeExpr::Intersection(Box::new(current), Box::new(exp));
        }
        current
    }
}
