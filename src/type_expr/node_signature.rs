//! Node signatures—the function-like type for nodes in a graph.
//!
//! A [`NodeSignature`] describes the generic parameters, inputs, outputs, and default types
//! of a node. It is analogous to a function signature in a programming language.

use crate::{
    scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{
        ScopePortal, TypeExpr, TypeExprScope, TypeExprValidationError, Unscoped,
        node_signature::{port_types::PortTypes, type_parameters::TypeParameters},
        subtyping::{DetailedSupertypeDiagnostics, NoSupertypeDiagnostics, SupertypeResult},
    },
};
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::DiGraph;
use std::collections::{BTreeMap, HashSet};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

pub mod candidate;
pub mod port_types;
pub mod type_parameters;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        rename_all = "camelCase",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize, S: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>, S: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, S: JsonSchema"))]
/// Function-like type for a node: generic parameters, inputs, outputs, and defaults.
/// Written in notation as `<T>(T) -> (T)` for the identity node.
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct NodeSignature<T: Type, S: TypeExprScope = Unscoped> {
    /// Generic parameters with their bounds and defaults.
    pub parameters: TypeParameters<T, S>,

    /// inputs and outputs are no PortTypes because having them TypeExpr
    /// Enables one to express `() -> Never` which is the node signature
    /// assignable to all other node signatures. Useful for inferring what
    /// type a closure should be.
    pub inputs: TypeExpr<T, S>,

    pub outputs: TypeExpr<T, S>,

    /// Types that get used when there is no connection to the input.
    /// These get ignored for nested NodeSignatures.
    pub default_input_types: BTreeMap<usize, TypeExpr<T, S>>,

    /// Tags this node *provides* to downstream consumers.
    ///
    /// - `Some(set)` — the node carries exactly the listed tags.
    /// - `None` — the node carries *every* tag (universal set), making it a
    ///   supertype of all other tag sets.
    ///
    /// Tags propagate forward through the graph: a node's effective tag set
    /// is the intersection of its own `tags` with every incoming edge's tags.
    ///
    /// Defaults to `Some(∅)` (no tags).
    ///
    /// `u32` is used instead of `usize` for wasm compatibility (JS has 32-bit
    /// integers).
    pub tags: Option<HashSet<u32>>,

    /// Tags this node *requires* from every incoming edge.
    ///
    /// During validation, each incoming edge's source tags are checked against
    /// this set. Any tag present here but missing from an incoming edge
    /// produces a [`TagMissing`](crate::validation::ValidationErrorKind::TagMissing)
    /// error.
    ///
    /// Defaults to `∅` (no requirements).
    pub required_tags: HashSet<u32>,
}

impl<T: Type, S: TypeExprScope> Default for NodeSignature<T, S> {
    fn default() -> Self {
        Self {
            parameters: TypeParameters::default(),
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::new())),
            outputs: TypeExpr::PortTypes(Box::new(PortTypes::new())),
            default_input_types: BTreeMap::new(),
            tags: Some(HashSet::new()),
            required_tags: HashSet::new(),
        }
    }
}

impl<T: Type, S: TypeExprScope> NodeSignature<T, S> {
    /// Sets the provided tags to `Some(tags)`. See [`NodeSignature::tags`].
    pub fn with_tags(self, tags: HashSet<u32>) -> Self {
        Self { tags: Some(tags), ..self }
    }

    /// Sets the required tags. See [`NodeSignature::required_tags`].
    pub fn with_required_tags(self, required_tags: HashSet<u32>) -> Self {
        Self { required_tags, ..self }
    }

    pub fn with_default_input_types(self, default_input_types: BTreeMap<usize, TypeExpr<T, S>>) -> Self {
        Self { default_input_types, ..self }
    }
}

impl<T: Type> NodeSignature<T> {
    pub fn supertype_of_all() -> Self {
        Self { inputs: TypeExpr::Any, outputs: TypeExpr::Never, ..Default::default() }
    }
}

impl<T: Type> NodeSignature<T, ScopePortal<T>> {
    /// Returns true if `self` could be replaced by `child` in a
    /// graph without invalidating any types. This is a convenience
    /// wrapper for the internal machinery. Needs ownership because
    /// the signature has to get wrapped in a type expression.
    pub fn supertype_of(self, child: NodeSignature<T, ScopePortal<T>>) -> SupertypeResult<NoSupertypeDiagnostics> {
        let parent_expr = TypeExpr::NodeSignature(Box::new(self));
        let child_expr = TypeExpr::NodeSignature(Box::new(child));

        let global_scope = ScopePointer::new_root();

        parent_expr.supertype_of(&child_expr, &global_scope, &global_scope)
    }

    pub fn supertype_of_detailed(
        self,
        child: NodeSignature<T, ScopePortal<T>>,
    ) -> SupertypeResult<DetailedSupertypeDiagnostics<T>> {
        let parent_expr = TypeExpr::NodeSignature(Box::new(self));
        let child_expr = TypeExpr::NodeSignature(Box::new(child));

        let global_scope = ScopePointer::new_root();

        parent_expr.supertype_of_detailed(&child_expr, &global_scope, &global_scope)
    }

    /// # Validates that
    /// - Parameters don't contain cycles.
    /// - Referenced parameters are defined.
    pub fn validate(&self, scope: &ScopePointer<T>) -> Result<(), TypeExprValidationError> {
        validate_type_parameters(&self.parameters)
            .map_err(|CyclicReferenceError| TypeExprValidationError::CyclicReference)?;
        let mut scope = Scope::new_child(scope);
        for (ident, param) in &self.parameters {
            scope.define(*ident, param.clone());
        }
        let scope = ScopePointer::new(scope);
        for param in self.parameters.values() {
            param.bound.as_ref().map(|bound| bound.validate(&scope)).transpose()?;
            param.default.as_ref().map(|default| default.validate(&scope)).transpose()?;
        }
        self.inputs.validate(&scope)?;
        self.outputs.validate(&scope)?;
        for default_type in self.default_input_types.values() {
            default_type.validate(&scope)?;
        }
        Ok(())
    }
}

impl<T: Type> NodeSignature<T, ScopePortal<T>> {
    /// Normalizes type parameters in inputs, outputs, and parameter bounds/defaults.
    pub fn normalize(&self, scope: &ScopePointer<T>) -> NodeSignature<T, ScopePortal<T>> {
        NodeSignature {
            parameters: self.parameters.iter().map(|(ident, param)| (*ident, param.normalize(scope))).collect(),
            inputs: self.inputs.normalize(scope),
            outputs: self.outputs.normalize(scope),
            ..self.clone()
        }
    }
}

pub struct CyclicReferenceError;

/// # Validates that
/// the parameters don't contain cycles as in <T extends U, U extends T>() -> ()
pub fn validate_type_parameters<T: Type>(
    parameters: &BTreeMap<LocalParamID, TypeParameter<T, ScopePortal<T>>>,
) -> Result<(), CyclicReferenceError> {
    let mut graph = DiGraph::new();
    let mut ident_to_idx = BTreeMap::new();
    for ident in parameters.keys() {
        let node_idx = graph.add_node(*ident);
        ident_to_idx.insert(*ident, node_idx);
    }

    for (ident, param) in parameters {
        let source_idx = ident_to_idx.get(ident).unwrap();
        let all_referenced = param
            .bound
            .iter()
            .chain(param.default.iter())
            .flat_map(|expr| expr.collect_references_type_params().into_iter());
        for referenced in all_referenced {
            let Some(target_idx) = ident_to_idx.get(&referenced) else { continue };
            graph.add_edge(*source_idx, *target_idx, ());
        }
    }
    if is_cyclic_directed(&graph) { Err(CyclicReferenceError) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::parse::sig;

    #[test]
    fn test_validate_sig_cyclic() {
        let signature = sig("<T extends U, U extends T>(T) -> (U)");
        let result = signature.validate(&ScopePointer::new_root());
        assert_eq!(result, Err(TypeExprValidationError::CyclicReference));
    }

    #[test]
    fn test_validate_sig_param_extends_itself() {
        let signature = sig("<#1, #8 extends #8 = AnySI>() -> ()");
        let result = signature.validate(&ScopePointer::new_root());
        assert_eq!(result, Err(TypeExprValidationError::CyclicReference));
    }

    #[test]
    fn test_validate_sig_invalid_ref() {
        let signature = sig("<T>(T) -> (U)");
        let result = signature.validate(&ScopePointer::new_root());
        assert_eq!(result, Err(TypeExprValidationError::UnknownVar(LocalParamID::from("U"))));
    }

    /// Cyclic params on a NESTED NodeSignature used to slip through validation
    /// because `TypeExpr::validate` only checked the outer expr's parameters.
    /// Without this check, `infer`/`normalize` would later recurse forever
    /// through the `TypeParameter` arms when following the bound.
    #[test]
    fn test_validate_nested_sig_param_extends_itself_in_input() {
        let signature = sig("(<#8 extends #8>() -> ()) -> ()");
        let result = signature.validate(&ScopePointer::new_root());
        assert_eq!(result, Err(TypeExprValidationError::CyclicReference));
    }

    /// Cyclic params on a nested NodeSignature sitting in a `default_input_types`
    /// slot used to slip through entirely because `NodeSignature::validate` never
    /// recursed into `default_input_types`.
    #[test]
    fn test_validate_nested_sig_param_extends_itself_in_default() {
        let signature = sig("(x: Any = (<#8 extends #8>() -> ())) -> ()");
        let result = signature.validate(&ScopePointer::new_root());
        assert_eq!(result, Err(TypeExprValidationError::CyclicReference));
    }
}
