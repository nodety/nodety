use crate::{
    nodety::inference::{Flow, Flows, InferenceConfig, InferenceDirection, InferenceStep},
    scope::{GlobalParameterId, LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{
        conditional::Conditional,
        node_signature::{
            CyclicReferenceError, NodeSignature, port_types::PortTypes, validate_type_parameters,
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
pub mod intersections;
pub mod node_signature;
pub mod normalization;
pub mod subtyping;

mod private {
    pub trait Sealed {}
}

pub trait TypeExprScope: private::Sealed + Clone {}

#[derive(Debug, Clone)]
pub struct ScopePortal<T: Type> {
    portal: ScopePointer<T>,
}

impl<T: Type> PartialEq for ScopePortal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.portal == other.portal
    }
}

pub type ScopedTypeExpr<T> = TypeExpr<T, ScopePortal<T>>;

/// In the future this could contain information about the portal scope.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
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
        tag = "type",
        content = "data",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize, S: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, S: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(
    feature = "json-schema",
    schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, S: JsonSchema")
)]
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

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
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

    /// Creates a union type from at least one type.
    /// If only one type is given, returns that type.
    pub fn from_unions(first: Self, following: Vec<Self>) -> Self {
        let mut current = first;
        for exp in following {
            current = TypeExpr::Union(Box::new(current), Box::new(exp));
        }
        current
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
    pub fn references(
        &self,
        needle: &HashSet<GlobalParameterId<T>>,
        scope: &ScopePointer<T>,
    ) -> bool {
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
                let global_id = GlobalParameterId {
                    scope: var_scope,
                    local_id: *param,
                };
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
    ) -> (
        Cow<'a, Self>,
        Option<&'a BTreeMap<LocalParamID, TypeParameter<T, ScopePortal<T>>>>,
    ) {
        match self {
            Self::NodeSignature(sig) if !sig.parameters.is_empty() => (
                Cow::Owned(TypeExpr::NodeSignature(Box::new(NodeSignature {
                    parameters: BTreeMap::new(),
                    ..*sig.clone()
                }))),
                Some(&sig.parameters),
            ),
            _ => (Cow::Borrowed(self), None),
        }
    }

    /// Calls walker for all types that are a "top level" union in self. Check out TypeExpr::traverse_mut for more infos
    /// Always visits
    pub fn traverse_union_mut(
        &mut self,
        scope: &ScopePointer<T>,
        walker: &mut impl FnMut(&mut Self, &ScopePointer<T>),
    ) {
        self.traverse_mut(
            scope,
            &mut |type_expr, scope, is_top_level_union| {
                if is_top_level_union {
                    walker(type_expr, scope);
                }
            },
            true,
        );
    }

    /// Traverses this expression for all "top level" unions.
    /// All non union expressions are considered leafs that are given to walker but not walked any further.
    pub fn traverse_union_non_context_sensitive<'a>(&'a self, walker: &mut impl FnMut(&'a Self)) {
        match self {
            Self::Union(a, b) => {
                a.traverse_union_non_context_sensitive(walker);
                b.traverse_union_non_context_sensitive(walker);
            }
            _ => walker(self),
        }
    }

    /// Calls walker for all types that are a "top level" union in self. Check out (TypeExpr::traverse_mut)[Self::traverse_mut] for more details.
    /// Always visits at least one type expr.
    pub fn traverse_union(
        &self,
        scope: &ScopePointer<T>,
        walker: &mut impl FnMut(&Self, &ScopePointer<T>),
    ) {
        self.traverse(
            scope,
            &mut |type_expr, scope, is_top_level_union| {
                if !is_top_level_union {
                    return;
                }
                if let TypeExpr::TypeParameter(param, _infer) = type_expr
                    && scope.is_inferred(param)
                {
                    // If the param is inferred, self.traverse will look it up and call this walker again.
                    return;
                }
                walker(type_expr, scope);
            },
            true,
        );
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
                if conditional.t_test.contains_uninferred(scope)
                    || conditional.t_test_bound.contains_uninferred(scope)
                {
                    could_widen = true;
                }
            },
            true,
        );
        could_widen
    }

    /// Mutable version of `traverse` with the exception, that the inferred
    /// types of type parameters don't get visited because they are immutable.
    /// # Walker
    /// (type_expr, scope, is_top_level_union) -> ()
    /// # Unions
    /// is_top_level_union is true if the type_expr itself is not a union but a type that
    /// is part of the started types union (One type is always a union).
    ///
    /// for T | null | keyof T
    /// - T | null | keyof T: false
    /// - T: true
    /// - null: true
    /// - keyof T: true
    /// - T: false
    pub fn traverse_mut(
        &mut self,
        scope: &ScopePointer<T>,
        walker: &mut impl FnMut(&mut Self, &ScopePointer<T>, bool),
        is_top_level_union: bool,
    ) {
        let self_is_union = matches!(self, Self::Union(_, _));
        walker(
            self,
            scope,
            if self_is_union {
                false
            } else {
                is_top_level_union
            },
        );

        let (_, scope) = self.build_uninferred_child_scope(scope);

        match self {
            Self::Operation { a, b, .. } => {
                a.traverse_mut(&scope, walker, false);
                b.traverse_mut(&scope, walker, false);
            }

            Self::Index { expr, index } => {
                expr.traverse_mut(&scope, walker, false);
                index.traverse_mut(&scope, walker, false);
            }

            Self::KeyOf(expr) => expr.traverse_mut(&scope, walker, false),

            Self::Union(a, b) => {
                a.traverse_mut(&scope, walker, is_top_level_union);
                b.traverse_mut(&scope, walker, is_top_level_union);
            }

            Self::Intersection(a, b) => {
                a.traverse_mut(&scope, walker, false);
                b.traverse_mut(&scope, walker, false);
            }

            Self::Type(_) => (),

            Self::TypeParameter(_, _) => (), // Type variables are immutable!

            Self::Constructor { parameters, .. } => parameters
                .values_mut()
                .for_each(|p| p.traverse_mut(&scope, walker, false)),

            Self::NodeSignature(sig) => {
                sig.inputs.traverse_mut(&scope, walker, false);
                sig.outputs.traverse_mut(&scope, walker, false);
                sig.parameters
                    .values_mut()
                    .flat_map(|param| param.bound.iter_mut().chain(param.default.iter_mut()))
                    .for_each(|t| t.traverse_mut(&scope, walker, false));
            }

            Self::PortTypes(pt) => {
                pt.iter_mut()
                    .for_each(|t| t.traverse_mut(&scope, walker, false));
            }

            Self::Conditional(conditional) => {
                conditional.t_test.traverse_mut(&scope, walker, false);
                conditional.t_test_bound.traverse_mut(&scope, walker, false);
                conditional.t_then.traverse_mut(&scope, walker, false);
                conditional.t_else.traverse_mut(&scope, walker, false);
            }

            Self::ScopePortal {
                expr,
                scope: ScopePortal { portal },
            } => {
                expr.traverse_mut(portal, walker, false);
            }

            Self::Any => (),
            Self::Never => (),
        }
    }

    pub fn traverse(
        &self,
        scope: &ScopePointer<T>,
        walker: &mut impl FnMut(&Self, &ScopePointer<T>, bool),
        is_top_level_union: bool,
    ) {
        let self_is_union = matches!(self, Self::Union(_, _));
        walker(
            self,
            scope,
            if self_is_union {
                false
            } else {
                is_top_level_union
            },
        );

        let (_, scope) = self.build_uninferred_child_scope(scope);

        match self {
            Self::Operation { a, b, .. } => {
                a.traverse(&scope, walker, false);
                b.traverse(&scope, walker, false);
            }

            Self::Index { expr, index } => {
                expr.traverse(&scope, walker, false);
                index.traverse(&scope, walker, false);
            }

            Self::KeyOf(expr) => expr.traverse(&scope, walker, false),

            Self::Union(a, b) => {
                a.traverse(&scope, walker, is_top_level_union);
                b.traverse(&scope, walker, is_top_level_union);
            }

            Self::Intersection(a, b) => {
                a.traverse(&scope, walker, false);
                b.traverse(&scope, walker, false);
            }

            Self::Type(_) => (),

            Self::TypeParameter(param, _infer) => {
                let Some((inferred, inferred_scope)) = scope.lookup_inferred(param) else {
                    return;
                };
                inferred.traverse(&inferred_scope, walker, is_top_level_union);
            }

            Self::Constructor { parameters, .. } => parameters
                .values()
                .for_each(|p| p.traverse(&scope, walker, false)),

            Self::NodeSignature(sig) => {
                sig.inputs.traverse(&scope, walker, false);
                sig.outputs.traverse(&scope, walker, false);
                sig.parameters
                    .values()
                    .flat_map(|param| param.bound.iter().chain(param.default.iter()))
                    .for_each(|t| t.traverse(&scope, walker, false));
            }

            Self::PortTypes(pt) => {
                pt.iter().for_each(|t| t.traverse(&scope, walker, false));
            }

            Self::Conditional(conditional) => {
                conditional.t_test.traverse(&scope, walker, false);
                conditional.t_test_bound.traverse(&scope, walker, false);
                conditional.t_then.traverse(&scope, walker, false);
                conditional.t_else.traverse(&scope, walker, false);
            }

            Self::ScopePortal {
                expr,
                scope: ScopePortal { portal },
            } => {
                expr.traverse(portal, walker, false);
            }

            Self::Any => (),
            Self::Never => (),
        }
    }

    /// Used for candidate search.
    /// if infer_other is true, `self` will be used to infer parameters in `other`.
    fn traverse_parallel(
        &self,
        other: &Self,
        own_scope: &ScopePointer<T>,
        other_scope: &ScopePointer<T>,
        infer_other: bool,
        walker: &mut impl FnMut(
            &Self,            // own_type
            &Self,            // other_type
            &ScopePointer<T>, // current own scope
            &ScopePointer<T>, // current other scope (potentially inferred from `self`)
        ),
    ) {
        walker(self, other, own_scope, other_scope);

        let (own, own_scope) = self.build_uninferred_child_scope(own_scope);
        let (other, other_scope) = if infer_other {
            other.build_inferred_child_scope(own.as_ref(), other_scope, &own_scope)
        } else {
            other.build_uninferred_child_scope(other_scope)
        };

        match (own.as_ref(), other.as_ref()) {
            // Unions first so that during candidate collection type type type params get visited by all union variants before being looked up.
            (Self::Union(own_a, own_b), other) => {
                own_a.traverse_parallel(other, &own_scope, &other_scope, infer_other, walker);
                own_b.traverse_parallel(other, &own_scope, &other_scope, infer_other, walker);
            }

            (own, Self::Union(other_a, other_b)) => {
                own.traverse_parallel(other_a, &own_scope, &other_scope, infer_other, walker);
                own.traverse_parallel(other_b, &own_scope, &other_scope, infer_other, walker);
            }

            (Self::Operation { a, b, operator }, other) => {
                let a_normalized = a.normalize(&own_scope);
                let b_normalized = b.normalize(&own_scope);
                T::operation(&a_normalized, operator, &b_normalized).traverse_parallel(
                    other,
                    &own_scope,
                    &other_scope,
                    infer_other,
                    walker,
                );
            }
            (own, Self::Operation { a, b, operator }) => {
                let a_normalized = a.normalize(&other_scope);
                let b_normalized = b.normalize(&other_scope);
                own.traverse_parallel(
                    &T::operation(&a_normalized, operator, &b_normalized),
                    &other_scope,
                    &own_scope,
                    infer_other,
                    walker,
                );
            }

            (Self::TypeParameter(own_param, _infer), other) => {
                let Some((own_inferred, own_inferred_scope)) = own_scope.lookup_inferred(own_param)
                else {
                    return;
                };
                own_inferred.traverse_parallel(
                    other,
                    &own_inferred_scope,
                    &other_scope,
                    infer_other,
                    walker,
                );
            }
            (own, Self::TypeParameter(other_param, _infer)) => {
                let Some((other_inferred, other_inferred_scope)) =
                    other_scope.lookup_inferred(other_param)
                else {
                    return;
                };
                own.traverse_parallel(
                    &other_inferred,
                    &own_scope,
                    &other_inferred_scope,
                    infer_other,
                    walker,
                );
            }

            (Self::KeyOf(own_expr), other) => {
                let Some((keyof, keyof_scope)) = own_expr.keyof(&own_scope) else {
                    return;
                };
                keyof.traverse_parallel(other, &keyof_scope, &other_scope, infer_other, walker);
            }

            (own, Self::KeyOf(other_expr)) => {
                let Some((keyof, keyof_scope)) = other_expr.keyof(&other_scope) else {
                    return;
                };
                own.traverse_parallel(&keyof, &own_scope, &keyof_scope, infer_other, walker);
            }

            (Self::Intersection(own_a, own_b), other) => {
                if let Some((own_intersection, own_intersection_scope)) =
                    Self::intersection(own_a, own_b, &own_scope, &own_scope)
                {
                    own_intersection.traverse_parallel(
                        other,
                        &own_intersection_scope,
                        &other_scope,
                        infer_other,
                        walker,
                    );
                }
            }

            (own, Self::Intersection(other_a, other_b)) => {
                if let Some((other_intersection, other_intersection_scope)) =
                    Self::intersection(other_a, other_b, &other_scope, &other_scope)
                {
                    own.traverse_parallel(
                        &other_intersection,
                        &own_scope,
                        &other_intersection_scope,
                        infer_other,
                        walker,
                    );
                }
            }

            (Self::Type(_), Self::Type(_)) => (),

            (
                Self::Constructor {
                    parameters: own_params,
                    inner: own_inner,
                },
                Self::Constructor {
                    parameters: other_params,
                    inner: other_inner,
                },
            ) => {
                if !own_inner.supertype_of(other_inner) {
                    return;
                }
                // Traverse over all common params
                for (key, own_param) in own_params {
                    if let Some(other_param) = other_params.get(key) {
                        own_param.traverse_parallel(
                            other_param,
                            &own_scope,
                            &other_scope,
                            infer_other,
                            walker,
                        );
                    }
                }
            }

            (Self::NodeSignature(own_signature), Self::NodeSignature(other_signature)) => {
                own_signature.inputs.traverse_parallel(
                    &other_signature.inputs,
                    &own_scope,
                    &other_scope,
                    infer_other,
                    walker,
                );
                own_signature.outputs.traverse_parallel(
                    &other_signature.outputs,
                    &own_scope,
                    &other_scope,
                    infer_other,
                    walker,
                );
                // What to do with type parameters?
            }

            (Self::PortTypes(own_ports), Self::PortTypes(other_ports)) => {
                let max_arg_count = own_ports.ports.len().max(other_ports.ports.len()) + 1;
                // Each port index (ports+varg) that is present in both is visited at least once.
                let mut i = 0;
                while let (Some(own_port), Some(other_port)) =
                    (own_ports.get_port_type(i), other_ports.get_port_type(i))
                {
                    own_port.traverse_parallel(
                        other_port,
                        &own_scope,
                        &other_scope,
                        infer_other,
                        walker,
                    );
                    i += 1;
                    if i >= max_arg_count {
                        break; // In case both have variadic ports.
                    }
                }
            }

            (Self::Index { expr, index }, other) => {
                let Some((own_idx, own_idx_scope)) = expr.index(index, &own_scope, &own_scope)
                else {
                    return;
                };
                own_idx.traverse_parallel(other, &own_idx_scope, &other_scope, infer_other, walker)
            }
            (own, Self::Index { expr, index }) => {
                let Some((other_idx, other_idx_scope)) =
                    expr.index(index, &other_scope, &other_scope)
                else {
                    return;
                };
                own.traverse_parallel(
                    &other_idx,
                    &own_scope,
                    &other_idx_scope,
                    infer_other,
                    walker,
                )
            }

            (Self::Conditional(own_conditional), other) => {
                let Some(distributed) = own_conditional.distribute(&own_scope) else {
                    return;
                };
                distributed.traverse_parallel(other, &own_scope, &other_scope, infer_other, walker)
            }
            (own, Self::Conditional(other_conditional)) => {
                let Some(distributed) = other_conditional.distribute(&other_scope) else {
                    return;
                };
                own.traverse_parallel(&distributed, &own_scope, &other_scope, infer_other, walker)
            }

            // (Self::Conditional(own_conditional), other) => {
            //     use SupertypeResult::*;
            //     if let Some(distributed) = own_conditional.distribute(&own_scope) {
            //         return distributed.traverse_parallel(other, &own_scope, &other_scope, false, walker);
            //     }
            //     let traversal =
            //         match own_conditional.t_test_bound.supertype_of(&own_conditional.t_test, &own_scope, &own_scope) {
            //             Supertype => &own_conditional.t_then,
            //             Unrelated(_) => &own_conditional.t_else,
            //             Unknown => return,
            //         };
            //     traversal.traverse_parallel(other, &own_scope, &other_scope, false, walker)
            // }

            // (own, Self::Conditional(other_conditional)) => {
            //     use SupertypeResult::*;
            //     if let Some(distributed) = other_conditional.distribute(&other_scope) {
            //         return own.traverse_parallel(&distributed, &own_scope, &other_scope, false, walker);
            //     }
            //     let traversal = match other_conditional.t_test_bound.supertype_of(
            //         &other_conditional.t_test,
            //         &other_scope,
            //         &other_scope,
            //     ) {
            //         Supertype => &other_conditional.t_then,
            //         Unrelated(_) => &other_conditional.t_else,
            //         Unknown => return,
            //     };
            //     own.traverse_parallel(traversal, &own_scope, &other_scope, false, walker)
            // }
            (Self::ScopePortal { expr, scope }, other) => {
                expr.traverse_parallel(other, &scope.portal, &other_scope, infer_other, walker)
            }
            (own, Self::ScopePortal { expr, scope }) => {
                own.traverse_parallel(expr, &own_scope, &scope.portal, infer_other, walker)
            }

            (Self::Any | Self::Never, _) => (),
            (_, Self::Any | Self::Never) => (),
            (_, Self::PortTypes(_)) => (),
            (Self::PortTypes(_), _) => (),
            (Self::Constructor { .. }, Self::Type(_)) => (),
            (Self::Type(_), Self::Constructor { .. }) => (),
            (Self::Type(_), Self::NodeSignature(_)) => (),
            (Self::Constructor { .. }, Self::NodeSignature(_)) => (),
            (Self::NodeSignature(_), Self::Type(_)) => (),
            (Self::NodeSignature(_), Self::Constructor { .. }) => (),
        }
    }

    /// Computes `self[index_type]`
    ///
    /// # Returns
    /// Some((indexed type, scope))
    ///
    /// If index_type is no legal index type for the type, returns Any.
    ///
    /// or None if
    /// - the index type is unknown due to uninferred vars.
    /// - Intersection or Union with distinct scopes.
    pub fn index(
        &self,
        index_type: &ScopedTypeExpr<T>,
        own_scope: &ScopePointer<T>,
        index_scope: &ScopePointer<T>,
    ) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        match self {
            Self::Type(inst) => Some((
                inst.index(None, &index_type.normalize(index_scope)),
                ScopePointer::clone(own_scope),
            )),
            Self::Constructor { inner, parameters } => Some((
                inner.index(Some(parameters), &index_type.normalize(index_scope)),
                ScopePointer::clone(own_scope),
            )),

            // see tsReference.ts
            // @todo test this
            // Distributes over the union
            Self::Union(a, b) => {
                let (a_idx, a_scope) = a.index(index_type, own_scope, index_scope)?;
                let (b_idx, b_scope) = b.index(index_type, own_scope, index_scope)?;
                Some((
                    Self::Union(
                        Box::new(Self::ScopePortal {
                            expr: Box::new(a_idx),
                            scope: ScopePortal { portal: a_scope },
                        }),
                        Box::new(Self::ScopePortal {
                            expr: Box::new(b_idx),
                            scope: ScopePortal { portal: b_scope },
                        }),
                    ),
                    ScopePointer::clone(own_scope),
                ))
            }

            // see tsReference.ts
            // @todo test this
            // Distributes over the intersection
            Self::Intersection(a, b) => {
                let (a_idx, a_scope) = a.index(index_type, own_scope, index_scope)?;
                let (b_idx, b_scope) = b.index(index_type, own_scope, index_scope)?;

                Some((
                    Self::Intersection(
                        Box::new(Self::ScopePortal {
                            expr: Box::new(a_idx),
                            scope: ScopePortal { portal: a_scope },
                        }),
                        Box::new(Self::ScopePortal {
                            expr: Box::new(b_idx),
                            scope: ScopePortal { portal: b_scope },
                        }),
                    ),
                    ScopePointer::clone(own_scope),
                ))
            }

            Self::Operation { a, b, operator } => {
                let a_normalized = a.normalize(own_scope);
                let b_normalized = b.normalize(own_scope);
                T::operation(&a_normalized, operator, &b_normalized).index(
                    index_type,
                    own_scope,
                    index_scope,
                )
            }

            Self::TypeParameter(param, _infer) => {
                // Was:
                // if let Some((bound, scope)) = own_scope.lookup_bound(param) {
                // But in the case:      <T>                 <C>
                //                       | T['abc'] | ----- | C  |
                //
                // C will get inferred using the (bound of T)['abc'] Even when T is not yet inferred.
                if let Some((inferred, scope)) = own_scope.lookup_inferred(param) {
                    inferred.index(index_type, &scope, index_scope)
                } else {
                    None
                }
            }
            Self::ScopePortal { expr, scope } => expr.index(index_type, &scope.portal, index_scope),
            // These can't be indexed.
            Self::NodeSignature(_) => Some((Self::Any, ScopePointer::clone(own_scope))),
            Self::PortTypes(_) => Some((Self::Any, ScopePointer::clone(own_scope))),
            Self::Conditional { .. } => Some((Self::Any, ScopePointer::clone(own_scope))),
            Self::Any => Some((Self::Any, ScopePointer::clone(own_scope))),
            Self::Index { .. } => Some((Self::Any, ScopePointer::clone(own_scope))),
            Self::KeyOf(_) => Some((Self::Any, ScopePointer::clone(own_scope))),
            Self::Never => Some((Self::Any, ScopePointer::clone(own_scope))),
        }
    }

    /// Returns the keys of the constructor fields if this is a constructor or is inferred to be a constructor. None otherwise.
    /// The returned expression is guaranteed not to contain any context sensitive types.
    /// # Returns
    /// `None` if
    /// - the key type is unknown due to uninferred vars.
    /// - Intersection or Union with distinct scopes.
    pub fn keyof(&self, scope: &ScopePointer<T>) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        // Normalize here so Index, keyof and TypeParameter don't need to get handled by this function.
        match self {
            Self::Type(inst) => Some((inst.key_type(None), ScopePointer::clone(scope))),

            Self::Constructor { inner, parameters } => {
                // Parameters should have been normalized by caller
                Some((inner.key_type(Some(parameters)), ScopePointer::clone(scope)))
            }

            // See tsReference.ts
            // @Todo: Test this
            // keyof (A|B) = keyof A & keyof B
            // This is not exactly what typescript does but close enough.
            Self::Union(a, b) => {
                let (keyof_a, keyof_a_scope) = a.keyof(scope)?;
                let (keyof_b, keyof_b_scope) = b.keyof(scope)?;
                if keyof_a.is_never_forever(&keyof_a_scope) {
                    return Some((keyof_b, keyof_b_scope));
                }
                if keyof_b.is_never_forever(&keyof_b_scope) {
                    return Some((keyof_a, keyof_a_scope));
                }
                Self::intersection(&keyof_a, &keyof_b, &keyof_a_scope, &keyof_b_scope)
            }

            // See tsReference.ts
            // @todo test this
            Self::Intersection(a, b) => {
                if a.is_never_forever(scope) || b.is_never_forever(scope) {
                    return Some((Self::Never, ScopePointer::clone(scope)));
                }
                let (keyof_a, keyof_a_scope) = a.keyof(scope)?;
                let (keyof_b, keyof_b_scope) = b.keyof(scope)?;
                Some((
                    Self::Union(
                        Box::new(Self::ScopePortal {
                            expr: Box::new(keyof_a),
                            scope: ScopePortal {
                                portal: keyof_a_scope,
                            },
                        }),
                        Box::new(Self::ScopePortal {
                            expr: Box::new(keyof_b),
                            scope: ScopePortal {
                                portal: keyof_b_scope,
                            },
                        }),
                    ),
                    ScopePointer::clone(scope),
                ))
            }

            Self::Operation { a, b, operator } => {
                let a_normalized = a.normalize(scope);
                let b_normalized = b.normalize(scope);
                T::operation(&a_normalized, operator, &b_normalized).keyof(scope)
            }

            Self::TypeParameter(param, _infer) => {
                // Was:
                // if let Some((bound, scope)) = scope.lookup_bound(param) {
                // But in the case:      <T>                 <C>
                //                       |   keyof T | ----- | C  |
                //
                // C will get inferred using the keyof(bound of T) Even when T is not yet inferred.
                if let Some((inferred, scope)) = scope.lookup_inferred(param) {
                    inferred.keyof(&scope)
                } else {
                    None
                }
            }
            Self::ScopePortal { expr, scope } => expr.keyof(&scope.portal),

            Self::KeyOf(expr) => expr.keyof(scope),

            Self::Index { expr, index } => {
                let (index_type, index_scope) = expr.index(index, scope, scope)?;
                index_type.keyof(&index_scope)
            }

            Self::Any => Some((T::keyof_any(), ScopePointer::clone(scope))),
            Self::NodeSignature(node_signature) => {
                // Customized behavior (defaults to never)
                Some((
                    T::keyof_node_signature(node_signature.as_ref()),
                    ScopePointer::clone(scope),
                ))
            }
            Self::PortTypes(_) => Some((Self::Never, ScopePointer::clone(scope))),
            // @todo
            Self::Conditional { .. } => Some((Self::Never, ScopePointer::clone(scope))),
            Self::Never => Some((Self::Never, ScopePointer::clone(scope))),
        }
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
    pub fn build_uninferred_child_scope<'a>(
        &'a self,
        scope: &ScopePointer<T>,
    ) -> (Cow<'a, Self>, ScopePointer<T>) {
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
            to_infer.insert(GlobalParameterId {
                scope: ScopePointer::clone(&own_scope),
                local_id: *ident,
            });
        }

        let flows = Flows {
            flows: vec![Flow {
                source,
                target: self_without_params.as_ref(),
                source_scope: ScopePointer::clone(source_scope),
                target_scope: ScopePointer::clone(&own_scope),
            }],
        };
        flows.infer(InferenceConfig {
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
        });
        (self_without_params, own_scope)
    }

    /// If the root expression does not have any parameters, returns a borrowed
    /// reference to self. Otherwise, returns an owned copy of self with all
    /// parameters removed (only one level).
    pub fn without_params<'a>(&'a self) -> Cow<'a, Self> {
        match self {
            Self::NodeSignature(sig) if !sig.parameters.is_empty() => {
                Cow::Owned(Self::NodeSignature(Box::new(NodeSignature {
                    parameters: BTreeMap::new(),
                    ..*sig.clone()
                })))
            }
            expr => Cow::Borrowed(expr),
        }
    }
}
