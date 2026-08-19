use crate::{
    scope::ScopePointer,
    r#type::Type,
    type_expr::{AsScopePortal, ErasedScopePortal, ScopePortal, ScopedTypeExpr, TypeExpr, TypeExprScope},
};
use std::fmt::Debug;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// PartialEq and Eq ignore diagnostics.
#[derive(Debug)]
pub enum SupertypeResult<Diagnostics> {
    Supertype,
    /// Definitely no supertype.
    Unrelated(Diagnostics),
    /// Can't be said for sure because of uninferred type variables.
    Unknown,
}

impl PartialEq for SupertypeResult<NoSupertypeDiagnostics> {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (SupertypeResult::Supertype, SupertypeResult::Supertype)
                | (SupertypeResult::Unrelated(_), SupertypeResult::Unrelated(_))
                | (SupertypeResult::Unknown, SupertypeResult::Unknown)
        )
    }
}

impl Eq for SupertypeResult<NoSupertypeDiagnostics> {}

/// Result compatible version of [SupertypeResult]
/// Used only inside [supertype_of_impl] to enable the try operator.
#[derive(Debug)]
enum NoSupertypeReason<Diagnostics> {
    /// Definitely no supertype.
    Unrelated(Diagnostics),
    /// Can't be said for sure because of uninferred type variables.
    /// @Todo: add the unknown type parameter
    Unknown,
}

impl<D> PartialEq for NoSupertypeReason<D> {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other), (Self::Unrelated(_), Self::Unrelated(_)) | (Self::Unknown, Self::Unknown))
    }
}

impl<D> SupertypeResult<D> {
    pub fn is_supertype(&self) -> bool {
        matches!(self, SupertypeResult::Supertype)
    }
}

impl<D> From<Result<(), NoSupertypeReason<D>>> for SupertypeResult<D> {
    fn from(result: Result<(), NoSupertypeReason<D>>) -> Self {
        match result {
            Ok(()) => SupertypeResult::Supertype,
            Err(NoSupertypeReason::Unrelated(d)) => SupertypeResult::Unrelated(d),
            Err(NoSupertypeReason::Unknown) => SupertypeResult::Unknown,
        }
    }
}

impl<Diagnostics> NoSupertypeReason<Diagnostics> {
    pub fn map_unrelated(self, mapper: impl FnOnce(Diagnostics) -> Diagnostics) -> Self {
        match self {
            Self::Unrelated(d) => Self::Unrelated(mapper(d)),
            other => other,
        }
    }
}

/// `new`/`add_layer` are generic over the scope of `parent`/`child` so that diagnostics can be
/// built directly from whatever scope kind is being compared (scoped or unscoped), without the
/// hot comparison path in [supertype_of_impl] ever needing to widen either side upfront. Erasing
/// down to [ErasedScopePortal] is a single [TypeExpr::map_scope_portals] pass regardless of the
/// input scope, so no extra conversion step is needed.
pub trait SupertypeDiagnostics<T: Type>: Debug {
    fn new<Sp: TypeExprScope, Sc: TypeExprScope>(
        parent: &TypeExpr<T, Sp>,
        child: &TypeExpr<T, Sc>,
        reason: Option<NoSupertypeLayerReason>,
    ) -> Self;

    fn new_empty() -> Self;

    fn add_layer<Sp: TypeExprScope, Sc: TypeExprScope>(
        self,
        parent: &TypeExpr<T, Sp>,
        child: &TypeExpr<T, Sc>,
        reason: Option<NoSupertypeLayerReason>,
    ) -> Self;
}

#[derive(Debug)]
pub struct NoSupertypeDiagnostics;

impl<T: Type> SupertypeDiagnostics<T> for NoSupertypeDiagnostics {
    fn new<Sp: TypeExprScope, Sc: TypeExprScope>(
        _parent: &TypeExpr<T, Sp>,
        _child: &TypeExpr<T, Sc>,
        _reason: Option<NoSupertypeLayerReason>,
    ) -> Self {
        NoSupertypeDiagnostics
    }

    fn new_empty() -> Self {
        Self
    }

    fn add_layer<Sp: TypeExprScope, Sc: TypeExprScope>(
        self,
        _parent: &TypeExpr<T, Sp>,
        _child: &TypeExpr<T, Sc>,
        _reason: Option<NoSupertypeLayerReason>,
    ) -> Self {
        // Discard data
        self
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone, PartialEq)]
pub enum NoSupertypeLayerReason {
    /// A type parameter failed to be looked up.
    UnknownTypeParam,
    /// NodeSignature: inputs varg/arity mismatch.
    NodeSignatureInputsVarg,
    /// NodeSignature: inputs (contravariant) failed.
    NodeSignatureInputs,
    /// NodeSignature: outputs (covariant) failed.
    NodeSignatureOutputs,
    /// NodeSignature: the parent's provided tags are not a superset of the
    /// child's provided tags. A parent must carry at least every tag the
    /// child carries (tags are covariant).
    NodeSignatureTags,
    /// NodeSignature: the child's required tags are not a superset of the
    /// parent's required tags. A child may require *more* tags but never
    /// *fewer* than the parent (required tags are contravariant).
    NodeSignatureRequiredTags,
    /// PortTypes: child has no varg but parent has.
    PortTypesVarg,
    /// PortTypes: arity mismatch (missing port).
    PortTypesArity,
    /// PortTypes: a port type comparison failed.
    PortTypesPort,
    /// Index access type comparison failed.
    Index,
    /// Constructor: arity mismatch (e.g. parent has params, child has none).
    ConstructorArity,
    /// Constructor: inner type or parameter comparison failed.
    ConstructorParam,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        rename_all = "camelCase",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone, PartialEq)]
pub struct NoSupertypeLayer<T: Type> {
    pub parent: TypeExpr<T, ErasedScopePortal>,
    pub child: TypeExpr<T, ErasedScopePortal>,
    pub reason: Option<NoSupertypeLayerReason>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        rename_all = "camelCase",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone, PartialEq)]
pub struct DetailedSupertypeDiagnostics<T: Type> {
    layers: Vec<NoSupertypeLayer<T>>,
}

impl<T: Type> SupertypeDiagnostics<T> for DetailedSupertypeDiagnostics<T> {
    fn new<Sp: TypeExprScope, Sc: TypeExprScope>(
        parent: &TypeExpr<T, Sp>,
        child: &TypeExpr<T, Sc>,
        reason: Option<NoSupertypeLayerReason>,
    ) -> Self {
        Self {
            layers: vec![NoSupertypeLayer {
                parent: parent.clone().map_scope_portals(&mut |_| ErasedScopePortal),
                child: child.clone().map_scope_portals(&mut |_| ErasedScopePortal),
                reason,
            }],
        }
    }

    fn new_empty() -> Self {
        Self { layers: vec![] }
    }

    fn add_layer<Sp: TypeExprScope, Sc: TypeExprScope>(
        mut self,
        parent: &TypeExpr<T, Sp>,
        child: &TypeExpr<T, Sc>,
        reason: Option<NoSupertypeLayerReason>,
    ) -> Self {
        self.layers.push(NoSupertypeLayer {
            parent: parent.clone().map_scope_portals(&mut |_| ErasedScopePortal),
            child: child.clone().map_scope_portals(&mut |_| ErasedScopePortal),
            reason,
        });
        self
    }
}

impl<T: Type, S: AsScopePortal<T>> TypeExpr<T, S> {
    /// More ergonomic wrapper for supertype_of if both the scope and supertype diagnostics are not important.
    ///
    /// Generic over the scope of both `self` and `child` independently (each may be scoped or
    /// unscoped): only the actual [`TypeExpr::ScopePortal`] nodes encountered while comparing get
    /// widened, so comparing two already-scoped expressions costs nothing extra.
    pub fn supertype_of_naive<S2: AsScopePortal<T>>(
        &self,
        child: &TypeExpr<T, S2>,
    ) -> SupertypeResult<NoSupertypeDiagnostics> {
        let scope = ScopePointer::new_root();
        self.supertype_of_impl::<NoSupertypeDiagnostics, S2>(child, &scope, &scope).into()
    }

    /// Generic over the scope of both `self` and `child` independently — see [Self::supertype_of_naive].
    pub fn supertype_of<S2: AsScopePortal<T>>(
        &self,
        child: &TypeExpr<T, S2>,
        parent_scope: &ScopePointer<T>,
        child_scope: &ScopePointer<T>,
    ) -> SupertypeResult<NoSupertypeDiagnostics> {
        self.supertype_of_impl::<NoSupertypeDiagnostics, S2>(child, parent_scope, child_scope).into()
    }

    /// Generic over the scope of both `self` and `child` independently — see [Self::supertype_of_naive].
    pub fn supertype_of_detailed<S2: AsScopePortal<T>>(
        &self,
        child: &TypeExpr<T, S2>,
        parent_scope: &ScopePointer<T>,
        child_scope: &ScopePointer<T>,
    ) -> SupertypeResult<DetailedSupertypeDiagnostics<T>> {
        self.supertype_of_impl::<DetailedSupertypeDiagnostics<T>, S2>(child, parent_scope, child_scope).into()
    }

    /// Determines wether or not other is a supertype of self.
    /// When encountering uninferred arguments on either side, false is returned.
    ///
    /// # Returns
    /// Result because that enables the try operator which comes in handy here.
    /// When [std::ops::Try] is stabilized, [NoSupertypeReason] could get replaced by SupertypeResult.
    fn supertype_of_impl<D: SupertypeDiagnostics<T>, S2: AsScopePortal<T>>(
        &self,
        child: &TypeExpr<T, S2>,
        parent_scope: &ScopePointer<T>,
        child_scope: &ScopePointer<T>,
    ) -> Result<(), NoSupertypeReason<D>> {
        use NoSupertypeReason::*;
        let (parent, parent_scope) = self.build_uninferred_child_scope(parent_scope);
        // Use the parent to infer the child's types.
        let (child, child_scope) = child.build_inferred_child_scope(parent.as_ref(), child_scope, &parent_scope);

        match (parent.as_ref(), child.as_ref()) {
            // Special types
            (TypeExpr::Any, _) => Ok(()),
            (parent, TypeExpr::Any) => match parent.is_any(&parent_scope) {
                None => Err(Unknown),
                Some(true) => Ok(()),
                Some(false) => Err(Unrelated(D::new(parent, child.as_ref(), None))),
            },

            (_, TypeExpr::Never) => Ok(()),
            (parent @ TypeExpr::Never, child) => match child.is_never(&child_scope) {
                None => Err(Unknown),
                Some(true) => Ok(()),
                Some(false) => Err(Unrelated(D::new(parent, child, None))),
            },

            // Type Param cases must be checked first because type parameters have to
            // get normalized all the way before being able to compare them.
            (
                parent @ TypeExpr::TypeParameter(parent_param, _infer1),
                child @ TypeExpr::TypeParameter(child_param, _infer2),
            ) => {
                // If any of them can't be be looked up fail quietly with unrelated.
                let Some((parent_registered, parent_param_scope)) = parent_scope.lookup(parent_param) else {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::UnknownTypeParam))));
                };
                let Some((child_registered, child_param_scope)) = child_scope.lookup(child_param) else {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::UnknownTypeParam))));
                };

                if parent_param_scope == child_param_scope && parent_param == child_param {
                    // both reference the same exact type variable
                    return Ok(());
                }

                // Check if the child boundary or inferred type is Never.
                let (child_boundary, child_boundary_scope) = child_registered.get_boundary(child_param_scope);
                if child_boundary.is_never(&child_boundary_scope).unwrap_or(false) {
                    return Ok(());
                }

                match (parent_registered.inferred(), child_registered.inferred()) {
                    (Some((parent_inferred, parent_inferred_scope)), Some((child_inferred, child_inferred_scope))) => {
                        parent_inferred.supertype_of_impl(
                            &child_inferred,
                            &parent_inferred_scope,
                            &child_inferred_scope,
                        )
                    }
                    (Some((parent_inferred, parent_inferred_scope)), None) => {
                        let (child_boundary, child_boundary_scope) = child_registered.get_boundary(child_param_scope);
                        if parent_inferred
                            .supertype_of(child_boundary.as_ref(), &parent_inferred_scope, &child_boundary_scope)
                            .is_supertype()
                        {
                            // if the child boundary is not yet inferred it could still be a subtype if its
                            // bound falls inside the parent parameter's bound. But if that fails, return
                            // Unknown because the result might still change when the child is inferred.
                            return Ok(());
                        }
                        parent_inferred.supertype_of_impl::<D, S2>(child, &parent_inferred_scope, &child_scope)
                    }
                    (None, Some((child_inferred, child_inferred_scope))) => parent
                        .supertype_of_impl::<D, ScopePortal<T>>(&child_inferred, &parent_scope, &child_inferred_scope),
                    (None, None) => Err(Unknown),
                }
            }

            (parent @ TypeExpr::TypeParameter(parent_param, _infer), child) => {
                let Some((parent_registered, _parent_param_scope)) = parent_scope.lookup(parent_param) else {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::UnknownTypeParam))));
                };
                if let Some((parent_inferred, parent_inferred_scope)) = parent_registered.inferred() {
                    parent_inferred
                        .supertype_of_impl::<D, S2>(child, &parent_inferred_scope, &child_scope)
                        .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, child, None)))
                } else {
                    Err(Unknown)
                }
            }

            (parent, child @ TypeExpr::TypeParameter(child_param, _infer)) => {
                let Some((child_registered, child_param_scope)) = child_scope.lookup(child_param) else {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::UnknownTypeParam))));
                };

                // Check if the child boundary or inferred type is Never.
                let (child_boundary, child_boundary_scope) = child_registered.get_boundary(child_param_scope);
                if child_boundary.is_never(&child_boundary_scope).unwrap_or(false) {
                    return Ok(());
                }

                if let Some((child_inferred, child_inferred_scope)) = child_registered.inferred() {
                    parent
                        .supertype_of_impl::<D, ScopePortal<T>>(&child_inferred, &parent_scope, &child_inferred_scope)
                        .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, child, None)))
                } else {
                    let (child_boundary, child_boundary_scope) = child_registered.get_boundary(child_param_scope);
                    parent
                        .supertype_of_impl::<D, ScopePortal<T>>(
                            child_boundary.as_ref(),
                            &parent_scope,
                            &child_boundary_scope,
                        )
                        .map_err(|_| Unknown)
                }
            }

            (parent, TypeExpr::ScopePortal { expr, scope }) => {
                parent.supertype_of_impl::<D, S2>(expr.as_ref(), &parent_scope, &scope.as_scope_portal().portal)
            }

            (TypeExpr::ScopePortal { expr, scope }, child) => {
                expr.supertype_of_impl::<D, S2>(child, &scope.as_scope_portal().portal, &child_scope)
            }

            (TypeExpr::Operation { a, b, operator }, child) => {
                let a_normalized = a.normalize(&parent_scope);
                let b_normalized = b.normalize(&parent_scope);
                T::operation(&a_normalized, operator, &b_normalized).supertype_of_impl::<D, S2>(
                    child,
                    &parent_scope,
                    &child_scope,
                )
            }

            (parent, TypeExpr::Operation { a, b, operator }) => {
                let a_normalized = a.normalize(&child_scope);
                let b_normalized = b.normalize(&child_scope);
                parent.supertype_of_impl::<D, ScopePortal<T>>(
                    &T::operation(&a_normalized, operator, &b_normalized),
                    &parent_scope,
                    &child_scope,
                )
            }

            (TypeExpr::KeyOf(parent_expr), child) => {
                let (keyof, keyof_scope) = parent_expr.keyof(&parent_scope).ok_or(Unknown)?;
                keyof.supertype_of_impl::<D, S2>(child, &keyof_scope, &child_scope)
            }

            (parent, child @ TypeExpr::KeyOf(child_expr)) => {
                let (keyof, keyof_scope) = child_expr.keyof(&child_scope).ok_or(Unknown)?;
                parent.supertype_of_impl::<D, ScopePortal<T>>(&keyof, &parent_scope, &keyof_scope).map_err(|e| {
                    e.map_unrelated(|d| d.add_layer(parent, child, Some(NoSupertypeLayerReason::UnknownTypeParam)))
                })
            }

            // self must be a supertype of both child_a and child_b.
            (parent @ TypeExpr::Union(_, _), TypeExpr::Union(child_a, child_b)) => {
                match (
                    parent.supertype_of_impl::<D, S2>(child_a, &parent_scope, &child_scope),
                    parent.supertype_of_impl::<D, S2>(child_b, &parent_scope, &child_scope),
                ) {
                    (Ok(()), Ok(())) => Ok(()),
                    (_, Err(Unknown)) | (Err(Unknown), _) => Err(Unknown),
                    (Err(Unrelated(e)), _) | (_, Err(Unrelated(e))) => Err(Unrelated(e)),
                }
            }

            // At least one of the parents must be a supertype of child.
            (parent @ TypeExpr::Union(parent_a, parent_b), child) => {
                match (
                    parent_a.supertype_of_impl::<D, S2>(child, &parent_scope, &child_scope),
                    parent_b.supertype_of_impl::<D, S2>(child, &parent_scope, &child_scope),
                ) {
                    (Ok(()), _) => Ok(()),
                    (_, Ok(())) => Ok(()),
                    (Err(Unknown), _) | (_, Err(Unknown)) => Err(Unknown),
                    (Err(Unrelated(e)), _) => Err(Unrelated(e.add_layer(parent, child, None))),
                }
            }

            // Parent must be supertype of both a and b.
            (parent, child @ TypeExpr::Union(child_a, child_b)) => {
                match (
                    parent.supertype_of_impl::<D, S2>(child_a, &parent_scope, &child_scope),
                    parent.supertype_of_impl::<D, S2>(child_b, &parent_scope, &child_scope),
                ) {
                    (Ok(()), Ok(())) => Ok(()),
                    (_, Err(Unknown)) | (Err(Unknown), _) => Err(Unknown),
                    (_, Err(Unrelated(e))) | (Err(Unrelated(e)), _) => Err(Unrelated(e.add_layer(parent, child, None))),
                }
            }

            (TypeExpr::Intersection(parent_a, parent_b), child) => {
                let (intersection, intersection_scope) =
                    TypeExpr::intersection(parent_a, parent_b, &parent_scope, &parent_scope).ok_or(Unknown)?;
                intersection
                    .supertype_of_impl::<D, S2>(child, &intersection_scope, &child_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(self, child, None)))
            }

            (parent, TypeExpr::Intersection(child_a, child_b)) => {
                let (intersection, intersection_scope) =
                    TypeExpr::intersection(child_a, child_b, &child_scope, &child_scope).ok_or(Unknown)?;
                parent
                    .supertype_of_impl::<D, ScopePortal<T>>(&intersection, &parent_scope, &intersection_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, &child, None)))
            }

            (TypeExpr::Conditional(conditional), child) => conditional
                .distribute(&parent_scope)
                .ok_or(Unknown)?
                .supertype_of_impl::<D, S2>(child, &parent_scope, &child_scope),

            (parent, TypeExpr::Conditional(conditional)) => parent.supertype_of_impl::<D, ScopePortal<T>>(
                &conditional.distribute(&child_scope).ok_or(Unknown)?,
                &parent_scope,
                &child_scope,
            ),

            (parent @ TypeExpr::NodeSignature(parent_sig), child @ TypeExpr::NodeSignature(child_sig)) => {
                // Inputs: parent (interface) with varg cannot be supertype of child (impl) when child
                // has more fixed ports than parent (child requires more args than parent's minimum).
                // Child with fewer or equal fixed ports is allowed (node may receive more inputs than it has).
                if let (TypeExpr::PortTypes(parent_in), TypeExpr::PortTypes(child_in)) =
                    (&parent_sig.inputs, &child_sig.inputs)
                    && parent_in.varg.is_some()
                    && child_in.varg.is_none()
                    && child_in.ports.len() > parent_in.ports.len()
                {
                    return Err(Unrelated(D::new(
                        parent,
                        child,
                        Some(NoSupertypeLayerReason::NodeSignatureInputsVarg),
                    )));
                }

                // contravariant
                child_sig.inputs.supertype_of_impl::<D, S>(&parent_sig.inputs, &child_scope, &parent_scope).map_err(
                    |e| {
                        e.map_unrelated(|d| {
                            d.add_layer(parent, child, Some(NoSupertypeLayerReason::NodeSignatureInputs))
                        })
                    },
                )?;

                // covariant
                parent_sig
                    .outputs
                    .supertype_of_impl::<D, S2>(&child_sig.outputs, &parent_scope, &child_scope)
                    .map_err(|e| {
                        e.map_unrelated(|d| {
                            d.add_layer(parent, child, Some(NoSupertypeLayerReason::NodeSignatureOutputs))
                        })
                    })?;

                // Tags: self can have more tags (provides more), but must have all of other's required tags
                if let Some(parent_tags) = &parent_sig.tags {
                    if let Some(child_tags) = &child_sig.tags {
                        if !parent_tags.is_superset(child_tags) {
                            return Err(Unrelated(D::new(
                                parent,
                                child,
                                Some(NoSupertypeLayerReason::NodeSignatureTags),
                            )));
                        }
                    } else {
                        // Child tags are All and parent tags are not => no supertype
                        return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::NodeSignatureTags))));
                    }
                } // else: parent tags are all the tags in the universe so supertype of all other tags.

                // Required tags: self can require less (more permissive) But can't require more.
                if !child_sig.required_tags.is_superset(&parent_sig.required_tags) {
                    return Err(Unrelated(D::new(
                        parent,
                        child,
                        Some(NoSupertypeLayerReason::NodeSignatureRequiredTags),
                    )));
                }
                Ok(())
            }

            (parent @ TypeExpr::PortTypes(parent_ports), child @ TypeExpr::PortTypes(child_ports)) => {
                if parent_ports.varg.is_some() && child_ports.varg.is_none() {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::PortTypesVarg))));
                }
                // + 1 to also capture the varg.
                let max_arg_count = parent_ports.ports.len().max(child_ports.ports.len()) + 1;
                for i in 0..max_arg_count {
                    let Some(parent_arg) = parent_ports.get_port_type(i) else {
                        break;
                    };
                    let Some(child_arg) = child_ports.get_port_type(i) else {
                        // if i >= parent_ports.ports.len() {
                        //     // Child has no vargs but parent has and this is one of them
                        //     // This is fine
                        //     return Ok(())
                        // }
                        return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::PortTypesArity))));
                    };
                    parent_arg.supertype_of_impl::<D, S2>(child_arg, &parent_scope, &child_scope).map_err(|e| {
                        e.map_unrelated(|d| d.add_layer(parent, child, Some(NoSupertypeLayerReason::PortTypesPort)))
                    })?;
                }
                Ok(())
            }

            (TypeExpr::Index { expr, index }, child) => {
                let (index_type, index_scope) = expr.index(index, &parent_scope, &parent_scope).ok_or(Unknown)?;
                index_type
                    .supertype_of_impl::<D, S2>(child, &index_scope, &child_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(self, child, Some(NoSupertypeLayerReason::Index))))
            }

            (parent, TypeExpr::Index { expr, index }) => {
                let (index_type, index_scope) = expr.index(index, &child_scope, &child_scope).ok_or(Unknown)?;
                parent
                    .supertype_of_impl::<D, ScopePortal<T>>(&index_type, &parent_scope, &index_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, &child, Some(NoSupertypeLayerReason::Index))))
            }

            // Last so that child is not TypeParameter or Union variant.
            (parent @ TypeExpr::Type(inst_parent), child @ TypeExpr::Type(inst_child)) => {
                if inst_parent.supertype_of(inst_child) { Ok(()) } else { Err(Unrelated(D::new(parent, child, None))) }
            }

            (parent @ TypeExpr::Type(inst_parent), child @ TypeExpr::Constructor { inner: inst_child, .. }) => {
                if inst_parent.supertype_of(inst_child) {
                    Ok(())
                } else {
                    Err(Unrelated(D::new(parent, child, None)))
                }
            }
            // Treat a constructor with no args the same as its inner type.
            (parent @ TypeExpr::Constructor { inner: inst_parent, parameters }, child @ TypeExpr::Type(inst_child)) => {
                if !parameters.is_empty() {
                    // Child has no parameters but parent has => No subtype
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::ConstructorArity))));
                }
                if inst_parent.supertype_of(inst_child) { Ok(()) } else { Err(Unrelated(D::new(parent, child, None))) }
            }

            (
                parent @ TypeExpr::Constructor { inner: parent_inner, parameters: parent_parameters },
                child @ TypeExpr::Constructor { inner: child_inner, parameters: child_parameters },
            ) => {
                if !parent_inner.supertype_of(child_inner) {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::ConstructorParam))));
                }
                for (ident, parent_param) in parent_parameters {
                    let Some(child_param) = child_parameters.get(ident) else {
                        if parent_param.is_optional_in_constructor(&parent_scope) {
                            continue;
                        }
                        return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::ConstructorArity))));
                    };
                    parent_param.supertype_of_impl::<D, S2>(child_param, &parent_scope, &child_scope).map_err(|e| {
                        e.map_unrelated(|d| d.add_layer(parent, child, Some(NoSupertypeLayerReason::ConstructorParam)))
                    })?;
                }
                Ok(())
            }

            (TypeExpr::NodeSignature(_), _) | (_, TypeExpr::NodeSignature(_)) => {
                Err(Unrelated(D::new(parent.as_ref(), child.as_ref(), None)))
            }
            (TypeExpr::PortTypes { .. }, _) | (_, TypeExpr::PortTypes { .. }) => {
                Err(Unrelated(D::new(parent.as_ref(), child.as_ref(), None)))
            }
        }
    }
}

impl<T: Type, S: AsScopePortal<T>> TypeExpr<T, S> {
    /// Determines wether or not the type is optional in a constructor.
    ///
    /// That is if it:
    /// - normalizes to either a concrete [Type] that returns true for [Type::optional_in_constructor]
    /// - or it normalizes to a union with at least one such type.
    ///
    /// Only ever called on a single (missing) constructor parameter, so widening it here (rather
    /// than threading `S` through `traverse`/`traverse_union`, which inline already-scoped inferred
    /// type parameters and so can't stay generic over `S`) is a small, bounded cost.
    pub fn is_optional_in_constructor(&self, scope: &ScopePointer<T>) -> bool {
        let scoped: ScopedTypeExpr<T> = self.clone().map_scope_portals(&mut |s: S| s.as_scope_portal().clone());
        let mut is_optional = false;
        scoped.traverse_union(scope, &mut |traversal_expr, traversal_scope| {
            if let Some(t) = traversal_expr.normalize_to_type(traversal_scope)
                && t.optional_in_constructor()
            {
                is_optional = true;
            }
        });
        is_optional
    }
}
