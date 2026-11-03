//! Validation of node graphs and type checking.
use crate::nodety::Nodety;
use crate::nodety::inference::Scopes;
use crate::scope::ScopePointer;
use crate::r#type::Type;
use crate::type_expr::{
    ConcreteTypeExpr, ScopedTypeExpr, TypeExpr, TypeExprValidationError,
    subtyping::{DetailedSupertypeDiagnostics, SupertypeResult},
    traversal::Variance,
};
use petgraph::Direction;
use petgraph::graph::NodeIndex;
use petgraph::visit::IntoEdgeReferences;
use petgraph::visit::{EdgeRef, IntoNodeReferences};
use std::collections::HashSet;
use std::fmt::Debug;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// Location of a validation error in the graph.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all_fields = "camelCase", tag = "type", content = "data"))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub enum GraphLocation {
    Edge(usize),
    InputPort { node_idx: usize, input_idx: usize },
    Node { node_idx: usize },
}

impl Default for GraphLocation {
    fn default() -> Self {
        Self::Edge(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        rename_all_fields = "camelCase",
        tag = "type",
        content = "data",
        bound(
            serialize = "T: Serialize, T::Operator: Serialize",
            deserialize = "T: Deserialize<'de>, T::Operator: Deserialize<'de>"
        )
    )
)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(
    feature = "json-schema",
    schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, crate::type_expr::ErasedScopedTypeRef<T>: JsonSchema")
)]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub enum ValidationErrorKind<T: Type> {
    /// A flow gives a type parameter a type outside of the parameter's bound, so no type can be inferred for it.
    /// The diagnostics have the bound as parent and the type the flow gives (or its arm outside of the bound) as child.
    ///
    /// Reported where the flow ends (an edge, or the input port of a default type) instead of
    /// [Self::InsufficientlyInferredTypes], which stays for parameters that no flow determines.
    TypeParameterBoundsExceeded(DetailedSupertypeDiagnostics<T>),
    MultipleEdgesOnOneInput,
    EdgeMissingOnInput,
    EdgeHasNoSourceOutput,
    /// There is a type parameter that is not yet inferred, but needed to validate a flow.
    /// This can happen when there is no flow to infer the parameter from, or the parameter's bound isn't inferred.
    /// A flow that violates the bound is reported as [Self::TypeParameterBoundsExceeded].
    InsufficientlyInferredTypes,
    NonPortTypesIO,
    TypeExprValidationError(TypeExprValidationError),
    TypeMismatch(DetailedSupertypeDiagnostics<T>),
    /// The node has effects that its parent neither has itself nor handles, or that exceed the bound given to
    /// [Nodety::validate_with_effect_bound] for nodes without a parent.
    UnhandledEffects(DetailedSupertypeDiagnostics<T>),
}

#[derive(Debug, PartialEq, Clone)]
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
#[cfg_attr(
    feature = "json-schema",
    schemars(bound = "T: JsonSchema, T::Operator: JsonSchema, crate::type_expr::ErasedScopedTypeRef<T>: JsonSchema")
)]
/// A validation error with its location and kind.
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct ValidationError<T: Type> {
    pub location: GraphLocation,
    pub kind: ValidationErrorKind<T>,
}

impl<T: Type> Nodety<T> {
    /// # Validates
    /// - All required inputs either have a default port or an edge.
    ///   Inputs whose type is optional (see [TypeExpr::is_optional_in_constructor]) are not required.
    /// - All inputs have at most one edge
    /// - Variadic inputs are continuous. Or only interrupted by default types
    /// - The effects of all nodes are within the effects their parent allows (see
    ///   [NodeSignature::handles](crate::type_expr::node_signature::NodeSignature::handles)). The effects of nodes
    ///   without a parent are unbounded, see [Self::validate_with_effect_bound].
    ///
    /// # Does not validate
    /// - That there are no edges without a matching input (if you need to validate this, you must do it yourself)
    pub fn validate(&self, scopes: &Scopes<T>) -> Vec<ValidationError<T>> {
        self.validate_impl(scopes, None)
    }

    /// Like [Self::validate], but the effects of nodes without a parent must also be within `effect_bound`.
    pub fn validate_with_effect_bound(
        &self,
        scopes: &Scopes<T>,
        effect_bound: &ConcreteTypeExpr<T>,
    ) -> Vec<ValidationError<T>> {
        self.validate_impl(scopes, Some(effect_bound))
    }

    fn validate_impl(&self, scopes: &Scopes<T>, effect_bound: Option<&ConcreteTypeExpr<T>>) -> Vec<ValidationError<T>> {
        let mut errors: Vec<ValidationError<T>> = Vec::new();

        errors.extend(self.validate_effects(scopes, effect_bound));

        errors.extend(self.validate_required_inputs_set(scopes));

        errors.extend(self.validate_types(scopes));

        errors
    }

    /// # Validates
    /// - All edges have an output
    /// - All outputs are super types of their corresponding input
    /// - All applied default types are sub types of their input
    fn validate_types(&self, scopes: &Scopes<T>) -> Vec<ValidationError<T>> {
        let mut errors = Vec::new();
        let mut populated: HashSet<(NodeIndex, usize)> = HashSet::new();
        // Validate edges
        for edge in self.program.edge_references() {
            populated.insert((edge.target(), edge.weight().target_port));

            let Some(source_scope) = scopes.get(&edge.source()) else {
                errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::InsufficientlyInferredTypes,
                });
                continue;
            };
            let Some(target_scope) = scopes.get(&edge.target()) else {
                errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::InsufficientlyInferredTypes,
                });
                continue;
            };

            let source_node = &self.program[edge.source()];
            let target_node = &self.program[edge.target()];

            // inputs and outputs have to get normalized first in case they are type params

            let source_outputs_scoped = source_node.signature.outputs.clone().into_scoped();

            let Some(source_ports) = source_outputs_scoped.get_port_types(source_scope) else {
                errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::NonPortTypesIO,
                });
                continue;
            };
            let Some(source_port) = source_ports.get_port_type(edge.weight().source_port) else {
                errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::EdgeHasNoSourceOutput,
                });
                continue;
            };
            // No matching input ports are ignored here!
            let target_inputs_scoped = target_node.signature.inputs.clone().into_scoped();
            let Some(target_ports) = target_inputs_scoped.get_port_types(target_scope) else {
                errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::NonPortTypesIO,
                });
                continue;
            };
            let Some(target_port) = target_ports.get_port_type(edge.weight().target_port) else { continue };

            match target_port.supertype_of_detailed(source_port, target_scope, source_scope) {
                SupertypeResult::Supertype => (),
                SupertypeResult::Unrelated(d) => errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::TypeMismatch(d),
                }),
                SupertypeResult::Unknown => errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: unknown_flow_error(target_port, source_port, target_scope, source_scope),
                }),
            }
        }
        // Validate default types
        for (node_idx, node) in self.program.node_references() {
            for (input_idx, default_type) in &node.signature.default_input_types {
                if populated.contains(&(node_idx, *input_idx)) {
                    continue;
                }

                let TypeExpr::PortTypes(ports) = &node.signature.inputs else {
                    errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: *input_idx },
                        kind: ValidationErrorKind::NonPortTypesIO,
                    });
                    continue;
                };
                let Some(port_type) = ports.get_port_type(*input_idx) else {
                    continue;
                };

                let Some(scope) = scopes.get(&node_idx) else {
                    errors.push(ValidationError {
                        location: GraphLocation::InputPort { input_idx: *input_idx, node_idx: node_idx.index() },
                        kind: ValidationErrorKind::InsufficientlyInferredTypes,
                    });
                    continue;
                };

                let port_type = port_type.clone().into_scoped();
                let default_type = default_type.clone().into_scoped();
                match port_type.supertype_of_detailed(&default_type, scope, scope) {
                    SupertypeResult::Supertype => (), // all ok
                    SupertypeResult::Unrelated(d) => errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: *input_idx },
                        kind: ValidationErrorKind::TypeMismatch(d),
                    }),
                    SupertypeResult::Unknown => errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: *input_idx },
                        kind: unknown_flow_error(&port_type, &default_type, scope, scope),
                    }),
                }
            }
        }
        errors
    }

    /// # Validates
    /// 1. All inputs have at most one edge
    /// 2. All required inputs either have a default port or an edge
    /// 3. Variadic inputs are continuous. Or only interrupted by default types or optional inputs
    fn validate_required_inputs_set(&self, scopes: &Scopes<T>) -> Vec<ValidationError<T>> {
        let mut errors = Vec::new();
        for (node_idx, node) in self.program.node_references() {
            let mut populated: HashSet<usize> = HashSet::new();
            // 1
            for edge_ref in self.program.edges_directed(node_idx, Direction::Incoming) {
                if !populated.insert(edge_ref.weight().target_port) {
                    errors.push(ValidationError {
                        location: GraphLocation::InputPort {
                            node_idx: node_idx.index(),
                            input_idx: edge_ref.weight().target_port,
                        },
                        kind: ValidationErrorKind::MultipleEdgesOnOneInput,
                    });
                }
            }

            // todo: normalize inputs first
            // 2
            let TypeExpr::PortTypes(ports) = &node.signature.inputs else {
                errors.push(ValidationError {
                    location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: 0 },
                    kind: ValidationErrorKind::NonPortTypesIO,
                });
                continue;
            };
            let is_required = |port_idx: usize| {
                !populated.contains(&port_idx)
                    && !node.signature.default_input_types.contains_key(&port_idx)
                    && !scopes.get(&node_idx).is_some_and(|scope| {
                        ports.get_port_type(port_idx).is_some_and(|t| t.is_optional_in_constructor(scope))
                    })
            };
            for port_idx in 0..ports.ports.len() {
                if is_required(port_idx) {
                    errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: port_idx },
                        kind: ValidationErrorKind::EdgeMissingOnInput,
                    });
                }
            }

            // 3
            if let Some(max) = populated.iter().max() {
                for port_idx in (ports.ports.len())..=*max.min(&ports.max_len()) {
                    if is_required(port_idx) {
                        errors.push(ValidationError {
                            location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: port_idx },
                            kind: ValidationErrorKind::EdgeMissingOnInput,
                        });
                    }
                }
            }
        }
        errors
    }

    /// # Validates
    /// that the effects of every node are within the effects its parent allows: the parent's own effects and the
    /// effects it handles. Nodes without a parent are checked against `effect_bound` if there is one.
    fn validate_effects(
        &self,
        scopes: &Scopes<T>,
        effect_bound: Option<&ConcreteTypeExpr<T>>,
    ) -> Vec<ValidationError<T>> {
        let mut errors = Vec::new();
        let root_scope = ScopePointer::new_root();
        for (node_idx, node) in self.program.node_references() {
            if node.signature.effects == TypeExpr::Never {
                continue;
            }
            let location = GraphLocation::Node { node_idx: node_idx.index() };
            let insufficiently_inferred =
                ValidationError { location, kind: ValidationErrorKind::InsufficientlyInferredTypes };
            let Some(scope) = scopes.get(&node_idx) else {
                errors.push(insufficiently_inferred);
                continue;
            };
            let effects = node.signature.effects.clone().into_scoped();
            let result = match node.parent {
                Some(parent_idx) => {
                    let Some(parent_scope) = scopes.get(&parent_idx) else {
                        errors.push(insufficiently_inferred);
                        continue;
                    };
                    let parent = &self.program[parent_idx].signature;
                    let allowed = if parent.handles == TypeExpr::Never {
                        parent.effects.clone()
                    } else {
                        parent.effects.clone().union_with(parent.handles.clone())
                    };
                    allowed.supertype_of_detailed(&effects, parent_scope, scope)
                }
                None => match effect_bound {
                    Some(bound) => bound.supertype_of_detailed(&effects, &root_scope, scope),
                    None => continue,
                },
            };
            match result {
                SupertypeResult::Supertype => (),
                SupertypeResult::Unrelated(d) => {
                    errors.push(ValidationError { location, kind: ValidationErrorKind::UnhandledEffects(d) })
                }
                SupertypeResult::Unknown => errors.push(insufficiently_inferred),
            }
        }
        errors
    }
}

/// The error for a flow from `source` into `target` that can't be checked because `target` references uninferred
/// parameters.
///
/// Inference discards the candidates outside of a parameter's bound, so a parameter whose candidates all exceed it
/// stays uninferred. If the flow gives such a parameter a fully inferred type from below (the parameter must be a
/// supertype of it) that the parameter's fully inferred bound doesn't contain, no type for the parameter can make the
/// flow valid: that is [ValidationErrorKind::TypeParameterBoundsExceeded], with the diagnostics of the normalized bound
/// as parent and the candidate as child (like [ValidationErrorKind::TypeMismatch], for a union candidate the outermost
/// layer names the arm outside of the bound). Otherwise the parameter may just lack a flow to infer it from:
/// [ValidationErrorKind::InsufficientlyInferredTypes].
fn unknown_flow_error<T: Type>(
    target: &ScopedTypeExpr<T>,
    source: &ScopedTypeExpr<T>,
    target_scope: &ScopePointer<T>,
    source_scope: &ScopePointer<T>,
) -> ValidationErrorKind<T> {
    for (global_id, candidates) in target.collect_candidates_in_order(source, target_scope, source_scope, false, true) {
        let Some((var, param_scope)) = global_id.scope.lookup(&global_id.local_id) else {
            continue;
        };
        let Some(bound) = &var.parameter().bound else {
            continue;
        };
        if !var.is_uninferred() || bound.contains_uninferred(param_scope) {
            continue;
        }
        let bound = bound.normalize(param_scope);
        for candidate in candidates {
            if candidate.variance != Variance::Covariant || candidate.t.contains_uninferred(&candidate.scope) {
                continue;
            }
            if let SupertypeResult::Unrelated(d) =
                bound.supertype_of_detailed(&candidate.t, param_scope, &candidate.scope)
            {
                return ValidationErrorKind::TypeParameterBoundsExceeded(d);
            }
        }
    }
    ValidationErrorKind::InsufficientlyInferredTypes
}
