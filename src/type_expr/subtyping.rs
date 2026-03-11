use crate::{
    scope::ScopePointer,
    r#type::Type,
    type_expr::{ErasedScopePortal, ScopedTypeExpr, TypeExpr},
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

pub trait SupertypeDiagnostics<T: Type>: Debug {
    fn new(parent: &ScopedTypeExpr<T>, child: &ScopedTypeExpr<T>, reason: Option<NoSupertypeLayerReason>) -> Self;

    fn new_empty() -> Self;

    fn add_layer(
        self,
        parent: &ScopedTypeExpr<T>,
        child: &ScopedTypeExpr<T>,
        reason: Option<NoSupertypeLayerReason>,
    ) -> Self;
}

#[derive(Debug)]
pub struct NoSupertypeDiagnostics;

impl<T: Type> SupertypeDiagnostics<T> for NoSupertypeDiagnostics {
    fn new(_parent: &ScopedTypeExpr<T>, _child: &ScopedTypeExpr<T>, _reason: Option<NoSupertypeLayerReason>) -> Self {
        NoSupertypeDiagnostics
    }

    fn new_empty() -> Self {
        Self
    }

    fn add_layer(
        self,
        _parent: &ScopedTypeExpr<T>,
        _child: &ScopedTypeExpr<T>,
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
    serde(rename_all = "camelCase", bound(
        serialize = "T: Serialize, T::Operator: Serialize",
        deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>"
    ))
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
    serde(rename_all = "camelCase", bound(
        serialize = "T: Serialize, T::Operator: Serialize",
        deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>"
    ))
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[derive(Debug, Clone, PartialEq)]
pub struct DetailedSupertypeDiagnostics<T: Type> {
    layers: Vec<NoSupertypeLayer<T>>,
}

impl<T: Type> SupertypeDiagnostics<T> for DetailedSupertypeDiagnostics<T> {
    fn new(parent: &ScopedTypeExpr<T>, child: &ScopedTypeExpr<T>, reason: Option<NoSupertypeLayerReason>) -> Self {
        Self { layers: vec![NoSupertypeLayer { parent: parent.clone().into(), child: child.clone().into(), reason }] }
    }

    fn new_empty() -> Self {
        Self { layers: vec![] }
    }

    fn add_layer(
        mut self,
        parent: &ScopedTypeExpr<T>,
        child: &ScopedTypeExpr<T>,
        reason: Option<NoSupertypeLayerReason>,
    ) -> Self {
        self.layers.push(NoSupertypeLayer { parent: parent.clone().into(), child: child.clone().into(), reason });
        self
    }
}

impl<T: Type> ScopedTypeExpr<T> {
    /// More ergonomic wrapper for supertype_of if both the scope and supertype diagnostics are not important.
    pub fn supertype_of_naive(&self, child: &ScopedTypeExpr<T>) -> SupertypeResult<NoSupertypeDiagnostics> {
        let scope = ScopePointer::new_root();
        self.supertype_of_impl::<NoSupertypeDiagnostics>(child, &scope, &scope).into()
    }

    pub fn supertype_of(
        &self,
        child: &ScopedTypeExpr<T>,
        parent_scope: &ScopePointer<T>,
        child_scope: &ScopePointer<T>,
    ) -> SupertypeResult<NoSupertypeDiagnostics> {
        self.supertype_of_impl::<NoSupertypeDiagnostics>(child, parent_scope, child_scope).into()
    }

    pub fn supertype_of_detailed(
        &self,
        child: &ScopedTypeExpr<T>,
        parent_scope: &ScopePointer<T>,
        child_scope: &ScopePointer<T>,
    ) -> SupertypeResult<DetailedSupertypeDiagnostics<T>> {
        self.supertype_of_impl::<DetailedSupertypeDiagnostics<T>>(child, parent_scope, child_scope).into()
    }

    /// Determines wether or not other is a supertype of self.
    /// When encountering uninferred arguments on either side, false is returned.
    ///
    /// # Returns
    /// Result because that enables the try operator which comes in handy here.
    /// When [std::ops::Try] is stabilized, [NoSupertypeReason] could get replaced by SupertypeResult.
    fn supertype_of_impl<D: SupertypeDiagnostics<T>>(
        &self,
        child: &Self,
        parent_scope: &ScopePointer<T>,
        child_scope: &ScopePointer<T>,
    ) -> Result<(), NoSupertypeReason<D>> {
        use NoSupertypeReason::*;
        let (parent, parent_scope) = self.build_uninferred_child_scope(parent_scope);
        // Use the parent to infer the child's types.
        let (child, child_scope) = child.build_inferred_child_scope(parent.as_ref(), child_scope, &parent_scope);

        match (parent.as_ref(), child.as_ref()) {
            // Special types
            (Self::Any, _) => Ok(()),
            (parent, Self::Any) => match parent.is_any(&parent_scope) {
                None => Err(Unknown),
                Some(true) => Ok(()),
                Some(false) => Err(Unrelated(D::new(parent, child.as_ref(), None))),
            },
            (_, Self::Never) => Ok(()),
            (parent @ Self::Never, child) => match child.is_never(&child_scope) {
                None => Err(Unknown),
                Some(true) => Ok(()),
                Some(false) => Err(Unrelated(D::new(parent, child, None))),
            },

            // Type Param cases must be checked first because type parameters have to
            // get normalized all the way before being able to compare them.
            (
                parent @ Self::TypeParameter(parent_param, _infer1),
                child @ Self::TypeParameter(child_param, _infer2),
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
                        parent_inferred.supertype_of_impl::<D>(child, &parent_inferred_scope, &child_scope)
                    }
                    (None, Some((child_inferred, child_inferred_scope))) => {
                        parent.supertype_of_impl::<D>(&child_inferred, &parent_scope, &child_inferred_scope)
                    }
                    (None, None) => Err(Unknown),
                }
            }

            (parent @ Self::TypeParameter(parent_param, _infer), child) => {
                let Some((parent_registered, _parent_param_scope)) = parent_scope.lookup(parent_param) else {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::UnknownTypeParam))));
                };
                if let Some((parent_inferred, parent_inferred_scope)) = parent_registered.inferred() {
                    parent_inferred
                        .supertype_of_impl::<D>(child, &parent_inferred_scope, &child_scope)
                        .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, child, None)))
                } else {
                    Err(Unknown)
                }
            }

            (parent, child @ Self::TypeParameter(child_param, _infer)) => {
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
                        .supertype_of_impl::<D>(&child_inferred, &parent_scope, &child_inferred_scope)
                        .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, child, None)))
                } else {
                    let (child_boundary, child_boundary_scope) = child_registered.get_boundary(child_param_scope);
                    parent
                        .supertype_of_impl::<D>(child_boundary.as_ref(), &parent_scope, &child_boundary_scope)
                        .map_err(|_| Unknown)
                }
            }

            (parent, Self::ScopePortal { expr, scope }) => {
                parent.supertype_of_impl::<D>(expr, &parent_scope, &scope.portal)
            }

            (Self::ScopePortal { expr, scope }, child) => {
                expr.supertype_of_impl::<D>(child, &scope.portal, &child_scope)
            }

            (Self::Operation { a, b, operator }, child) => {
                let a_normalized = a.normalize(&parent_scope);
                let b_normalized = b.normalize(&parent_scope);
                T::operation(&a_normalized, operator, &b_normalized).supertype_of_impl::<D>(
                    child,
                    &parent_scope,
                    &child_scope,
                )
            }

            (parent, Self::Operation { a, b, operator }) => {
                let a_normalized = a.normalize(&child_scope);
                let b_normalized = b.normalize(&child_scope);
                parent.supertype_of_impl::<D>(
                    &T::operation(&a_normalized, operator, &b_normalized),
                    &parent_scope,
                    &child_scope,
                )
            }

            (Self::KeyOf(parent_expr), child) => {
                let (keyof, keyof_scope) = parent_expr.keyof(&parent_scope).ok_or(Unknown)?;
                keyof.supertype_of_impl::<D>(child, &keyof_scope, &child_scope)
            }

            (parent, child @ Self::KeyOf(child_expr)) => {
                let (keyof, keyof_scope) = child_expr.keyof(&child_scope).ok_or(Unknown)?;
                parent.supertype_of_impl::<D>(&keyof, &parent_scope, &keyof_scope).map_err(|e| {
                    e.map_unrelated(|d| d.add_layer(parent, child, Some(NoSupertypeLayerReason::UnknownTypeParam)))
                })
            }

            // self must be a supertype of both child_a and child_b.
            (parent @ Self::Union(_, _), Self::Union(child_a, child_b)) => {
                match (
                    parent.supertype_of_impl::<D>(child_a, &parent_scope, &child_scope),
                    parent.supertype_of_impl::<D>(child_b, &parent_scope, &child_scope),
                ) {
                    (Ok(()), Ok(())) => Ok(()),
                    (_, Err(Unknown)) | (Err(Unknown), _) => Err(Unknown),
                    (Err(Unrelated(e)), _) | (_, Err(Unrelated(e))) => Err(Unrelated(e)),
                }
            }

            // At least one of the parents must be a supertype of child.
            (parent @ Self::Union(parent_a, parent_b), child) => {
                match (
                    parent_a.supertype_of_impl::<D>(child, &parent_scope, &child_scope),
                    parent_b.supertype_of_impl::<D>(child, &parent_scope, &child_scope),
                ) {
                    (Ok(()), _) => Ok(()),
                    (_, Ok(())) => Ok(()),
                    (Err(Unknown), _) | (_, Err(Unknown)) => Err(Unknown),
                    (Err(Unrelated(e)), _) => Err(Unrelated(e.add_layer(parent, child, None))),
                }
            }

            // Parent must be supertype of both a and b.
            (parent, child @ Self::Union(child_a, child_b)) => {
                match (
                    parent.supertype_of_impl::<D>(child_a, &parent_scope, &child_scope),
                    parent.supertype_of_impl::<D>(child_b, &parent_scope, &child_scope),
                ) {
                    (Ok(()), Ok(())) => Ok(()),
                    (_, Err(Unknown)) | (Err(Unknown), _) => Err(Unknown),
                    (_, Err(Unrelated(e))) | (Err(Unrelated(e)), _) => Err(Unrelated(e.add_layer(parent, child, None))),
                }
            }

            (Self::Intersection(parent_a, parent_b), child) => {
                let (intersection, intersection_scope) =
                    Self::intersection(parent_a, parent_b, &parent_scope, &parent_scope).ok_or(Unknown)?;
                intersection
                    .supertype_of_impl::<D>(child, &intersection_scope, &child_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(self, child, None)))
            }

            (parent, Self::Intersection(child_a, child_b)) => {
                let (intersection, intersection_scope) =
                    Self::intersection(child_a, child_b, &child_scope, &child_scope).ok_or(Unknown)?;
                parent
                    .supertype_of_impl::<D>(&intersection, &parent_scope, &intersection_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, &child, None)))
            }

            (Self::Conditional(conditional), child) => conditional
                .distribute(&parent_scope)
                .ok_or(Unknown)?
                .supertype_of_impl::<D>(child, &parent_scope, &child_scope),

            (parent, Self::Conditional(conditional)) => parent.supertype_of_impl::<D>(
                &conditional.distribute(&child_scope).ok_or(Unknown)?,
                &parent_scope,
                &child_scope,
            ),

            (parent @ Self::NodeSignature(parent_sig), child @ Self::NodeSignature(child_sig)) => {
                // Inputs: parent (interface) with varg cannot be supertype of child (impl) when child
                // has more fixed ports than parent (child requires more args than parent's minimum).
                // Child with fewer or equal fixed ports is allowed (node may receive more inputs than it has).
                if let (Self::PortTypes(parent_in), Self::PortTypes(child_in)) = (&parent_sig.inputs, &child_sig.inputs)
                {
                    if parent_in.varg.is_some()
                        && child_in.varg.is_none()
                        && child_in.ports.len() > parent_in.ports.len()
                    {
                        return Err(Unrelated(D::new(
                            parent,
                            child,
                            Some(NoSupertypeLayerReason::NodeSignatureInputsVarg),
                        )));
                    }
                }

                // contravariant
                child_sig.inputs.supertype_of_impl::<D>(&parent_sig.inputs, &child_scope, &parent_scope).map_err(
                    |e| {
                        e.map_unrelated(|d| {
                            d.add_layer(parent, child, Some(NoSupertypeLayerReason::NodeSignatureInputs))
                        })
                    },
                )?;

                // covariant
                parent_sig.outputs.supertype_of_impl::<D>(&child_sig.outputs, &parent_scope, &child_scope).map_err(
                    |e| {
                        e.map_unrelated(|d| {
                            d.add_layer(parent, child, Some(NoSupertypeLayerReason::NodeSignatureOutputs))
                        })
                    },
                )?;

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

            (parent @ Self::PortTypes(parent_ports), child @ Self::PortTypes(child_ports)) => {
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
                    parent_arg.supertype_of_impl::<D>(child_arg, &parent_scope, &child_scope).map_err(|e| {
                        e.map_unrelated(|d| d.add_layer(parent, child, Some(NoSupertypeLayerReason::PortTypesPort)))
                    })?;
                }
                Ok(())
            }

            (Self::Index { expr, index }, child) => {
                let (index_type, index_scope) = expr.index(index, &parent_scope, &parent_scope).ok_or(Unknown)?;
                index_type
                    .supertype_of_impl::<D>(child, &index_scope, &child_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(self, child, Some(NoSupertypeLayerReason::Index))))
            }

            (parent, Self::Index { expr, index }) => {
                let (index_type, index_scope) = expr.index(index, &child_scope, &child_scope).ok_or(Unknown)?;
                parent
                    .supertype_of_impl::<D>(&index_type, &parent_scope, &index_scope)
                    .map_err(|e| e.map_unrelated(|d| d.add_layer(parent, &child, Some(NoSupertypeLayerReason::Index))))
            }

            // Last so that child is not TypeParameter or Union variant.
            (parent @ Self::Type(inst_parent), child @ Self::Type(inst_child)) => {
                if inst_parent.supertype_of(inst_child) { Ok(()) } else { Err(Unrelated(D::new(parent, child, None))) }
            }

            (parent @ Self::Type(inst_parent), child @ Self::Constructor { inner: inst_child, .. }) => {
                if inst_parent.supertype_of(inst_child) { Ok(()) } else { Err(Unrelated(D::new(parent, child, None))) }
            }
            // Treat a constructor with no args the same as its inner type.
            (parent @ Self::Constructor { inner: inst_parent, parameters }, child @ Self::Type(inst_child)) => {
                if !parameters.is_empty() {
                    // Child has no parameters but parent has => No subtype
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::ConstructorArity))));
                }
                if inst_parent.supertype_of(inst_child) { Ok(()) } else { Err(Unrelated(D::new(parent, child, None))) }
            }

            (
                parent @ Self::Constructor { inner: parent_inner, parameters: parent_parameters },
                child @ Self::Constructor { inner: child_inner, parameters: child_parameters },
            ) => {
                if !parent_inner.supertype_of(child_inner) {
                    return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::ConstructorParam))));
                }
                for (ident, parent_param) in parent_parameters {
                    let Some(child_param) = child_parameters.get(ident) else {
                        return Err(Unrelated(D::new(parent, child, Some(NoSupertypeLayerReason::ConstructorArity))));
                    };
                    parent_param.supertype_of_impl::<D>(child_param, &parent_scope, &child_scope).map_err(|e| {
                        e.map_unrelated(|d| d.add_layer(parent, child, Some(NoSupertypeLayerReason::ConstructorParam)))
                    })?;
                }
                Ok(())
            }

            (Self::NodeSignature(_), _) | (_, Self::NodeSignature(_)) => {
                Err(Unrelated(D::new(parent.as_ref(), child.as_ref(), None)))
            }
            (Self::PortTypes { .. }, _) | (_, Self::PortTypes { .. }) => {
                Err(Unrelated(D::new(parent.as_ref(), child.as_ref(), None)))
            }
        }
    }
}
