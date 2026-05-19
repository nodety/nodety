//! Cached wrapper around [`Nodety`] that returns cached inference scopes when the graph hasn't changed.
//!
//! The inference config (steps) is fixed at construction time and must stay the same for cache validity.

use crate::{
    TypeExpr,
    inference::{FlowSourceLocation, InferenceStep, Scopes, infer},
    nodety::{Edge, IntoNode, Node, Nodety, NodetyError, inference::InferenceConfig},
    scope::ScopePointer,
    r#type::Type,
    type_expr::Unscoped,
    validation::ValidationError,
};
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::StableDiGraph;
use std::cell::RefCell;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// A wrapper around [`Nodety`] that caches inference results.
///
/// When [`infer`](NodetyCached::infer) is called, it returns cached scopes if the graph
/// hasn't been modified since the last inference. The inference steps are fixed at
/// construction time.
pub struct NodetyCached<T: Type> {
    nodety: Nodety<T>,
    config: InferenceConfig<T>,
    cache: RefCell<Option<Scopes<T>>>,
}

impl<T: Type> Default for NodetyCached<T> {
    fn default() -> Self {
        Self::new(InferenceStep::default_steps())
    }
}

impl<T: Type> NodetyCached<T> {
    /// Creates a new cached nodety with the given inference steps.
    pub fn new(steps: Vec<InferenceStep>) -> Self {
        Self {
            nodety: Nodety::new(),
            config: InferenceConfig { steps, ..Default::default() },
            cache: RefCell::new(None),
        }
    }

    pub fn from_nodety(nodety: Nodety<T>, steps: Vec<InferenceStep>) -> Self {
        Self { nodety, config: InferenceConfig { steps, ..Default::default() }, cache: RefCell::new(None) }
    }

    /// Creates a new cached nodety with estimated capacity and the given inference steps.
    pub fn with_capacity(nodes: usize, edges: usize, steps: Vec<InferenceStep>) -> Self {
        Self {
            nodety: Nodety::with_capacity(nodes, edges),
            config: InferenceConfig { steps, ..Default::default() },
            cache: RefCell::new(None),
        }
    }

    fn invalidate_cache(&self) {
        *self.cache.borrow_mut() = None;
    }

    /// Adds a node to the graph.
    pub fn add_node(&mut self, node: impl IntoNode<T>) -> Result<NodeIndex, NodetyError> {
        let result = self.nodety.add_node(node)?;
        self.invalidate_cache();
        Ok(result)
    }

    /// Updates the node at `node_id` with new data.
    pub fn update_node(&mut self, node_id: NodeIndex, node: impl IntoNode<T>) -> Result<(), NodetyError> {
        self.nodety.update_node(node_id, node)?;
        self.invalidate_cache();
        Ok(())
    }

    /// Removes the node at `node_id` from the graph.
    pub fn remove_node(&mut self, node_id: NodeIndex) -> Result<(), NodetyError> {
        self.nodety.remove_node(node_id)?;
        self.invalidate_cache();
        Ok(())
    }

    /// Adds an edge from a source output port to a target input port.
    pub fn add_edge(&mut self, source: NodeIndex, target: NodeIndex, edge: Edge) -> Result<EdgeIndex, NodetyError> {
        let idx = self.nodety.add_edge(source, target, edge)?;
        self.invalidate_cache();
        Ok(idx)
    }

    /// Removes an edge and returns it if it existed.
    pub fn remove_edge(&mut self, edge_idx: EdgeIndex) -> Option<Edge> {
        self.nodety.remove_edge(edge_idx).inspect(|_| self.invalidate_cache())
    }

    /// Returns the node at `node_idx`.
    pub fn get_node(&self, node_idx: NodeIndex) -> Option<&Node<T, Unscoped>> {
        self.nodety.get_node(node_idx)
    }

    /// Returns the underlying graph.
    pub fn program(&self) -> &StableDiGraph<Node<T, Unscoped>, Edge> {
        self.nodety.program()
    }

    /// Returns the graph in Graphviz Dot notation.
    pub fn to_dot(&self) -> String {
        self.nodety.to_dot()
    }

    /// Infers the type parameters for all nodes in the program.
    ///
    /// Returns cached scopes if the graph hasn't been modified since the last inference.
    pub fn infer(&self) -> Scopes<T> {
        if let Some(cached) = self.cache.borrow().as_ref() {
            return cached.clone();
        }
        let result = self.nodety.infer(&self.config);
        *self.cache.borrow_mut() = Some(result.clone());
        result
    }

    pub fn discard_cache(&self) {
        *self.cache.borrow_mut() = None;
    }

    /// Validates the graph using the inferred types.
    pub fn validate(&self) -> Vec<ValidationError<T>> {
        if let Some(cached) = self.cache.borrow().as_ref() {
            return self.nodety.validate(cached);
        }
        let scopes = self.nodety.infer(&self.config);
        let result = self.nodety.validate(&scopes);
        *self.cache.borrow_mut() = Some(scopes);
        result
    }

    /// TODO: ScopePointer allow interior mutability!!!
    /// Infers the scope of a node. Allows specifying ports to exclude from inference.
    pub fn infer_node_scope(
        &self,
        node_idx: NodeIndex,
        exclude_input: Option<ExcludePorts>,
        exclude_output: Option<ExcludePorts>,
    ) -> Result<ScopePointer<T>, NodetyError> {
        if exclude_input.is_none() && exclude_output.is_none() {
            // If there are no exclude criteria, take advantage of the cache
            let scopes = self.infer();
            let Some(node_scope) = scopes.get(&node_idx) else { return Err(NodetyError::NodeNotFound) };
            return Ok(ScopePointer::clone(node_scope));
        }

        let scopes = self.nodety.build_scopes();

        let mut flows = self.nodety.collect_flows(&scopes);

        let prev_len = flows.len();

        let Some(node_signature) = self.nodety.get_node(node_idx) else { return Err(NodetyError::NodeNotFound) };

        let min_input_ports_len =
            if let TypeExpr::PortTypes(inputs) = &node_signature.signature.inputs { inputs.ports.len() } else { 0 };
        let min_output_ports_len =
            if let TypeExpr::PortTypes(outputs) = &node_signature.signature.outputs { outputs.ports.len() } else { 0 };

        flows.retain(|flow| {
            if flow.target_location.node_idx == node_idx {
                match exclude_input {
                    None => (),
                    Some(ExcludePorts::Index(idx)) if idx == flow.target_location.input_idx => return false,
                    Some(ExcludePorts::Vargs) if flow.target_location.input_idx >= min_input_ports_len => return false,
                    _ => (),
                };
            }
            let FlowSourceLocation::Output(flow_source_node_idx, output_idx) = flow.source_location else {
                return true;
            };
            if flow_source_node_idx == node_idx {
                match exclude_output {
                    None => (),
                    Some(ExcludePorts::Index(idx)) if idx == output_idx => return false,
                    Some(ExcludePorts::Vargs) if output_idx >= min_output_ports_len => return false,
                    _ => (),
                };
            }
            true
        });

        let changed = prev_len != flows.len();

        let scopes = if changed {
            let raw_flows = flows.into_iter().map(|flow| flow.flow).collect();
            infer(raw_flows, &self.config);
            scopes
        } else {
            // If there were no flows matching the exclude criteria, take advantage of the cache
            self.infer()
        };

        let Some(node_scope) = scopes.get(&node_idx) else { return Err(NodetyError::NodeNotFound) };

        Ok(ScopePointer::clone(node_scope))
    }

    pub fn inner(&self) -> &Nodety<T> {
        &self.nodety
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", content = "index"))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum ExcludePorts {
    /// Excludes the port at the given index
    Index(usize),
    /// Excludes all vargs
    Vargs,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        demo_type::DemoType,
        notation::parse::{expr, expr_u, sig_u},
    };
    use maplit::btreemap;

    #[test]
    pub fn test_normalize_type_from_parent() {
        let mut nodety = NodetyCached::<DemoType>::new(InferenceStep::default_steps());
        let map_node_idx = nodety.add_node(Node::new(sig_u("<I>(Array<I> = Array<Integer>) -> ()"))).unwrap();

        let input_node_idx =
            nodety.add_node(Node::new_child(sig_u("() -> (I)"), NodeIndex::from(map_node_idx))).unwrap();

        let scope = nodety.infer_node_scope(input_node_idx, Some(ExcludePorts::Index(0)), None).unwrap();

        let normalized = expr("I").normalize(&scope);

        assert_eq!(normalized, expr("Integer"));
    }

    #[test]
    pub fn test_normalize_ignore_varg_defaults() {
        let mut nodety = NodetyCached::<DemoType>::default();
        let sig = sig_u("<I>(...I) -> ()")
            .with_default_input_types(btreemap! {0usize => expr_u("Integer"), 1usize => expr_u("Integer")});
        let node_idx = nodety.add_node(Node::new(sig)).unwrap();

        let scope = nodety.infer_node_scope(node_idx, Some(ExcludePorts::Vargs), None).unwrap();
        let normalized = expr("I").normalize(&scope);
        assert_eq!(normalized, expr("I"));

        // Should still infer to integer because input #1 has a default
        let scope = nodety.infer_node_scope(node_idx, Some(ExcludePorts::Index(0)), None).unwrap();
        let normalized = expr("I").normalize(&scope);
        assert_eq!(normalized, expr("Integer"));

        let scope = nodety.infer_node_scope(node_idx, None, None).unwrap();
        let normalized = expr("I").normalize(&scope);
        assert_eq!(normalized, expr("Integer"));
    }
}
