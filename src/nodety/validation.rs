//! Validation of node graphs and type checking.
use crate::nodety::Nodety;
use crate::nodety::inference::Scopes;
use crate::r#type::Type;
use crate::type_expr::{
    TypeExpr, TypeExprValidationError,
    subtyping::{DetailedSupertypeDiagnostics, SupertypeResult},
};
use petgraph::Direction;
use petgraph::graph::NodeIndex;
use petgraph::visit::IntoEdgeReferences;
use petgraph::visit::{EdgeRef, IntoNodeReferences, Topo};
use std::collections::{HashMap, HashSet};
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
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema"))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub enum ValidationErrorKind<T: Type> {
    /// The inferred type for a type parameter is no child type of the parameter bound
    TypeParameterBoundsExceeded,
    /// An incoming edge is missing one or more tags that the target node
    /// requires (via [`NodeSignature::required_tags`](crate::type_expr::node_signature::NodeSignature::required_tags)).
    /// The set contains the specific tag ids that were absent.
    TagMissing(HashSet<u32>),
    MultipleEdgesOnOneInput,
    EdgeMissingOnInput,
    EdgeHasNoSourceOutput,
    /// There is a type parameter that is not yet inferred, but needed to validate a flow.
    /// This can happen when either there is no flow to infer the parameter from or
    /// The flows violate the parameters bounds.
    InsufficientlyInferredTypes,
    NonPortTypesIO,
    TypeExprValidationError(TypeExprValidationError),
    TypeMismatch(DetailedSupertypeDiagnostics<T>),
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
#[cfg_attr(feature = "json-schema", schemars(bound = "T: JsonSchema, T::Operator: JsonSchema"))]
/// A validation error with its location and kind.
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct ValidationError<T: Type> {
    pub location: GraphLocation,
    pub kind: ValidationErrorKind<T>,
}

impl<T: Type> Nodety<T> {
    /// # Validates
    /// - All required inputs either have a default port or an edge
    /// - All inputs have at most one edge
    /// - Variadic inputs are continuous. Or only interrupted by default types
    /// - All nodes receive their required tags
    ///
    /// # Does not validate
    /// - That there are no edges without a matching input (if you need to validate this, you must do it yourself)
    pub fn validate(&self, scopes: &Scopes<T>) -> Vec<ValidationError<T>> {
        let mut errors: Vec<ValidationError<T>> = Vec::new();

        errors.extend(self.validate_tags());

        errors.extend(self.validate_required_inputs_set());

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
            let source_node = &self.program[edge.source()];
            let target_node = &self.program[edge.target()];
            let TypeExpr::PortTypes(source_ports) = &source_node.signature.outputs else {
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
            let TypeExpr::PortTypes(target_ports) = &target_node.signature.inputs else {
                errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::NonPortTypesIO,
                });
                continue;
            };
            let Some(target_port) = target_ports.get_port_type(edge.weight().target_port) else {
                continue;
            };

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

            match target_port.clone().into_scoped().supertype_of_detailed(
                &source_port.clone().into_scoped(),
                target_scope,
                source_scope,
            ) {
                SupertypeResult::Supertype => (),
                SupertypeResult::Unrelated(d) => errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::TypeMismatch(d),
                }),
                SupertypeResult::Unknown => errors.push(ValidationError {
                    location: GraphLocation::Edge(edge.id().index()),
                    kind: ValidationErrorKind::InsufficientlyInferredTypes,
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

                match port_type.clone().into_scoped().supertype_of_detailed(
                    &default_type.clone().into_scoped(),
                    scope,
                    scope,
                ) {
                    SupertypeResult::Supertype => (), // all ok
                    SupertypeResult::Unrelated(d) => errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: *input_idx },
                        kind: ValidationErrorKind::TypeMismatch(d),
                    }),
                    SupertypeResult::Unknown => errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: *input_idx },
                        kind: ValidationErrorKind::InsufficientlyInferredTypes,
                    }),
                }
            }
        }
        errors
    }

    /// # Validates
    /// 1. All inputs have at most one edge
    /// 2. All required inputs either have a default port or an edge
    /// 3. Variadic inputs are continuous. Or only interrupted by default types
    fn validate_required_inputs_set(&self) -> Vec<ValidationError<T>> {
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

            // 2
            let TypeExpr::PortTypes(ports) = &node.signature.inputs else {
                errors.push(ValidationError {
                    location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: 0 },
                    kind: ValidationErrorKind::NonPortTypesIO,
                });
                continue;
            };
            for (port_idx, _port_type) in ports.ports.iter().enumerate() {
                if !populated.contains(&port_idx) && !node.signature.default_input_types.contains_key(&port_idx) {
                    errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: port_idx },
                        kind: ValidationErrorKind::EdgeMissingOnInput,
                    });
                }
            }

            // 3
            if let Some(max) = populated.iter().max() {
                let TypeExpr::PortTypes(input_ports) = &node.signature.inputs else {
                    errors.push(ValidationError {
                        location: GraphLocation::InputPort { node_idx: node_idx.index(), input_idx: 0 },
                        kind: ValidationErrorKind::NonPortTypesIO,
                    });
                    continue;
                };
                for port_idx in (input_ports.ports.len())..=*max.min(&input_ports.max_len()) {
                    if !populated.contains(&port_idx) && !node.signature.default_input_types.contains_key(&port_idx) {
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

    /// Validates that every node receives the tags it requires.
    ///
    /// Walks the graph in topological order, propagating each node's
    /// effective tag set (intersection of its own
    /// [`tags`](crate::type_expr::node_signature::NodeSignature::tags) with
    /// every incoming edge's tags). For each incoming edge, any tag in the
    /// node's [`required_tags`](crate::type_expr::node_signature::NodeSignature::required_tags)
    /// that is absent from the source's effective tags produces a
    /// [`TagMissing`](ValidationErrorKind::TagMissing) error on that edge.
    fn validate_tags(&self) -> Vec<ValidationError<T>> {
        let mut errors = Vec::new();
        let mut tags_cache: HashMap<NodeIndex, Option<HashSet<u32>>> = HashMap::new();
        let mut topo = Topo::new(&self.program);
        while let Some(node_idx) = topo.next(&self.program) {
            let node = &self.program[node_idx];
            let Some(mut node_tags) = node.signature.tags.clone() else {
                tags_cache.insert(node_idx, None);
                continue;
            };
            for edge_ref in self.program.edges_directed(node_idx, Direction::Incoming) {
                let Some(incoming_tags) = tags_cache.get(&edge_ref.source()).unwrap() else {
                    continue;
                };
                let missing_tags: HashSet<u32> =
                    node.signature.required_tags.difference(incoming_tags).copied().collect();
                // if let Some(node_tags)
                node_tags = node_tags.intersection(incoming_tags).copied().collect();
                if !missing_tags.is_empty() {
                    errors.push(ValidationError {
                        location: GraphLocation::Edge(edge_ref.id().index()),
                        kind: ValidationErrorKind::TagMissing(missing_tags),
                    });
                }
            }
            tags_cache.insert(node_idx, Some(node_tags));
        }
        errors
    }
}
