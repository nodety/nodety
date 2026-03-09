//! Directed graph of nodes and edges for type-checked programs.
//!
//! [`Nodety`] represents a program as a graph of [`NodeSignature`]s
//! with edges between their ports. It performs type inference and validation.

use crate::{
    scope::{LocalParamID, ScopePointer},
    r#type::Type,
    type_expr::{
        ScopePortal, TypeExpr, TypeExprScope, TypeExprValidationError, Unscoped,
        node_signature::NodeSignature,
    },
};
use petgraph::dot::Dot;
pub use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::StableDiGraph;
use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Debug, Display},
};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

pub mod inference;
pub mod validation;

mod private {
    pub trait Sealed {}
}

/// An edge connecting a source output port to a target input port.
#[derive(Debug, Clone)]
pub struct Edge {
    pub source_port: usize,
    pub target_port: usize,
}

/// A node in the nodety graph.
#[derive(Debug, Clone)]
pub struct Node<T: Type, S: TypeExprScope = Unscoped> {
    pub signature: NodeSignature<T, S>,
    /// Node index of the parent node if there is one.
    pub parent: Option<NodeIndex>,
    /// These will get inferred directly before inferring anything else. Setting
    /// this is required only when inference is ambiguous. Aka rusts "type annotations needed".
    pub type_hints: BTreeMap<LocalParamID, TypeExpr<T, S>>,
}

impl<T: Type> Node<T, Unscoped> {
    pub fn new(signature: NodeSignature<T, Unscoped>) -> Self {
        Self {
            signature,
            parent: None,
            type_hints: BTreeMap::new(),
        }
    }

    pub fn new_child(signature: NodeSignature<T, Unscoped>, parent: NodeIndex) -> Self {
        Self {
            signature,
            parent: Some(parent),
            type_hints: BTreeMap::new(),
        }
    }

    pub fn with_type_hints(
        self,
        type_hints: BTreeMap<LocalParamID, TypeExpr<T, Unscoped>>,
    ) -> Self {
        Self { type_hints, ..self }
    }
}

/// This could be represented by Into as well but having it in this trait
/// has the added benefit that `T` will stay inferred across `impl IntoNode<T>`.
/// With into, the T gets lost.
pub trait IntoNode<T: Type>: private::Sealed {
    fn into_node(self) -> Node<T, ScopePortal<T>>;
}

impl<T: Type> private::Sealed for Node<T, Unscoped> {}

impl<T: Type> private::Sealed for NodeSignature<T, Unscoped> {}

impl<T: Type> IntoNode<T> for NodeSignature<T, Unscoped> {
    fn into_node(self) -> Node<T, ScopePortal<T>> {
        Node {
            signature: self.into(),
            parent: None,
            type_hints: BTreeMap::new(),
        }
    }
}

impl<T: Type> IntoNode<T> for Node<T, Unscoped> {
    fn into_node(self) -> Node<T, ScopePortal<T>> {
        Node {
            signature: self.signature.into(),
            parent: self.parent,
            type_hints: self
                .type_hints
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl<T: Type> Default for Node<T, Unscoped> {
    fn default() -> Self {
        Node {
            signature: NodeSignature::default(),
            parent: None,
            type_hints: BTreeMap::new(),
        }
    }
}

impl<T: Type> From<Node<T, Unscoped>> for Node<T, ScopePortal<T>> {
    fn from(value: Node<T, Unscoped>) -> Self {
        Node {
            signature: value.signature.into(),
            parent: value.parent,
            type_hints: value
                .type_hints
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl<T: Type> From<NodeSignature<T, Unscoped>> for Node<T, ScopePortal<T>> {
    fn from(sig: NodeSignature<T, Unscoped>) -> Self {
        Node {
            signature: sig.into(),
            parent: None,
            type_hints: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub enum NodetyError {
    NodeHasChildren,
    ParentNotFound,
    NodeNotFound,
    CycleDetected,
    TypeExprValidationError(TypeExprValidationError),
}

impl Display for NodetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for NodetyError {}

type ChildrenMap = BTreeMap<NodeIndex, Vec<NodeIndex>>;

pub struct Nodety<T: Type> {
    program: StableDiGraph<Node<T, ScopePortal<T>>, Edge>,
    children: ChildrenMap,
}

impl<T: Type> Default for Nodety<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Type> Nodety<T> {
    /// Creates an empty graph with capacity of 0.
    pub fn new() -> Self {
        Self::with_capacity(0, 0)
    }

    /// Creates an empty graph with estimated capacity.
    pub fn with_capacity(nodes: usize, edges: usize) -> Self {
        Self {
            program: StableDiGraph::with_capacity(nodes, edges),
            children: BTreeMap::new(),
        }
    }

    /// Adds a node to the graph.
    ///
    /// **Hint:** `Into<Node<T, ScopePortal<T>>>` is implemented for `NodeSignature<T, Unscoped>` and `Node<T, Unscoped>`.
    ///
    /// # Errors
    /// - if the parent was updated to a non existing node
    /// - if the new parent would create a cycle
    /// - if the node signature is invalid (See [NodeSignature::validate](crate::NodeSignature::validate))
    ///
    ///  # Example
    /// ```
    /// # use nodety::{demo_type::DemoType, NodeSignature, Nodety, Node};
    /// # use std::str::FromStr;
    /// # use nodety::type_expr::{TypeExpr, node_signature::port_types::PortTypes};
    ///
    /// let mut nodety = Nodety::<DemoType>::new();
    ///
    /// // Adding a simple node using the notation
    /// let map_node = NodeSignature::from_str("<T, U>(Array<T>) -> (Array<U>)").unwrap();
    /// let map_node_idx = nodety.add_node(map_node).unwrap();
    ///
    /// // Adding a node without using the parser module
    /// let node = NodeSignature { inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Type(DemoType::Integer)]))), ..Default::default() };
    /// let _ = nodety.add_node(node).unwrap();
    ///
    /// // Adding a node as child of another node.
    /// // Here the map node has access to the T and U parameters of the node.
    /// let map_node = NodeSignature::from_str("(T) -> (U)").unwrap();
    /// let map_node_idx = nodety.add_node(Node{signature: map_node, parent: Some(map_node_idx), ..Default::default()}).unwrap();
    /// ```
    pub fn add_node(&mut self, node: impl IntoNode<T>) -> Result<NodeIndex, NodetyError> {
        let node: Node<T, ScopePortal<T>> = node.into_node();
        let node_scope = if let Some(parent) = node.parent {
            if !self.program.contains_node(parent) {
                return Err(NodetyError::ParentNotFound);
            }
            self.build_node_scope(parent)
        } else {
            ScopePointer::new_root()
        };
        node.signature
            .validate(&node_scope)
            .map_err(NodetyError::TypeExprValidationError)?;

        let parent = node.parent;
        let node_idx = self.program.add_node(node);
        if let Some(parent) = parent {
            self.register_child(parent, node_idx);
        }
        Ok(node_idx)
    }

    /// Updates the node at `node_id` with new data.
    ///
    /// **Hint:** `IntoNode<Node<T>>` is implemented for `NodeSignature<T>` and `Node<T>`.
    ///
    /// # Errors
    /// - if the node is not found in the graph
    /// - if the parent was updated to a non existing node
    /// - if the new parent would create a cycle
    /// - if the node signature is invalid (See [NodeSignature::validate](crate::NodeSignature::validate))
    pub fn update_node(
        &mut self,
        node_id: NodeIndex,
        node: impl IntoNode<T>,
    ) -> Result<(), NodetyError> {
        let node: Node<T, ScopePortal<T>> = node.into_node();

        if !self.program.contains_node(node_id) {
            return Err(NodetyError::NodeNotFound);
        }

        let node_scope = if let Some(parent) = self.program[node_id].parent {
            self.build_node_scope(parent)
        } else {
            ScopePointer::new_root()
        };

        node.signature
            .validate(&node_scope)
            .map_err(NodetyError::TypeExprValidationError)?;

        if let Some(new_parent) = node.parent {
            if !self.program.contains_node(new_parent) {
                return Err(NodetyError::ParentNotFound);
            }
            if self.would_create_cycle(node_id, new_parent) {
                return Err(NodetyError::CycleDetected);
            }
        }

        if let Some(old_parent) = self.program[node_id].parent {
            self.unregister_child(old_parent, node_id);
        }
        if let Some(new_parent) = node.parent {
            self.register_child(new_parent, node_id);
        }

        self.program[node_id] = node;
        Ok(())
    }

    /// Removes the node at `node_id` from the graph.
    ///
    /// # Errors
    /// - if the node has children
    pub fn remove_node(&mut self, node_id: NodeIndex) -> Result<(), NodetyError> {
        if self.children.get(&node_id).is_some_and(|c| !c.is_empty()) {
            return Err(NodetyError::NodeHasChildren);
        }
        if let Some(old_node) = self.program.remove_node(node_id) {
            if let Some(parent) = old_node.parent {
                self.unregister_child(parent, node_id);
            }
        }
        self.children.remove(&node_id);
        Ok(())
    }

    /// Adds an edge from a source output port to a target input port.
    pub fn add_edge(
        &mut self,
        source: NodeIndex,
        target: NodeIndex,
        source_port: usize,
        target_port: usize,
    ) -> EdgeIndex {
        self.program.add_edge(
            source,
            target,
            Edge {
                source_port,
                target_port,
            },
        )
    }

    /// Removes an edge and returns it if it existed.
    pub fn remove_edge(&mut self, edge_idx: EdgeIndex) -> Option<Edge> {
        self.program.remove_edge(edge_idx)
    }

    pub fn get_node(&self, node_idx: NodeIndex) -> Option<&Node<T, ScopePortal<T>>> {
        self.program.node_weight(node_idx)
    }

    /// Returns the underlying graph.
    pub fn program(&self) -> &StableDiGraph<Node<T, ScopePortal<T>>, Edge> {
        &self.program
    }

    /// Removes `child_id` from its parent's children list.
    fn unregister_child(&mut self, parent: NodeIndex, child_id: NodeIndex) {
        if let Some(children) = self.children.get_mut(&parent) {
            children.retain(|&id| id != child_id);
            if children.is_empty() {
                self.children.remove(&parent);
            }
        }
    }

    /// Registers `child_id` as a child of `parent`.
    fn register_child(&mut self, parent: NodeIndex, child_id: NodeIndex) {
        self.children.entry(parent).or_default().push(child_id);
    }

    /// Returns true if walking from `parent` up the parent chain would reach `descendant`.
    /// Used to detect cycles when setting a node's parent.
    fn would_create_cycle(&self, node_id: NodeIndex, new_parent: NodeIndex) -> bool {
        let mut current = new_parent;
        loop {
            if current == node_id {
                return true;
            }
            let Some(node) = self.program.node_weight(current) else {
                return false;
            };
            let Some(parent) = node.parent else {
                return false;
            };
            current = parent;
        }
    }

    /// Returns the graph in Graphviz Dot notation.
    ///
    /// Useful for debugging.
    pub fn to_dot(&self) -> String {
        format!("{:?}", Dot::new(&self.program))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_type::DemoType;
    use crate::notation::parse::sig_u;
    use crate::type_expr::node_signature::NodeSignature;

    #[test]
    fn test_parent_child_tracking() {
        let mut system = Nodety::<DemoType>::new();

        // Add root node (no parent)
        let root = system.add_node(sig_u("<T>() -> ()")).unwrap();

        // Add child with existing parent
        let child = system
            .add_node(Node::new_child(sig_u("(T) -> ()"), root))
            .unwrap();

        // Cannot remove root while it has children
        assert!(matches!(
            system.remove_node(root),
            Err(NodetyError::NodeHasChildren)
        ));

        // Can remove leaf (child has no children)
        assert!(system.remove_node(child).is_ok());
        assert!(system.remove_node(root).is_ok());
    }

    #[test]
    fn test_parent_must_exist() {
        let mut system = Nodety::<DemoType>::new();
        let _root = system.add_node(NodeSignature::default()).unwrap();

        // NodeIndex::from(99) is a non-existent parent
        let bad_node = Node::new_child(sig_u("() -> ()"), NodeIndex::from(99));
        assert!(matches!(
            system.add_node(bad_node),
            Err(NodetyError::ParentNotFound)
        ));
    }

    #[test]
    fn test_no_cycle_on_update() {
        let mut system = Nodety::<DemoType>::new();
        let a = system.add_node(NodeSignature::default()).unwrap();
        let b = system
            .add_node(Node::new_child(NodeSignature::default(), a))
            .unwrap();

        // Making A's parent B would create cycle A -> B -> A
        assert!(matches!(
            system.update_node(a, Node::new_child(NodeSignature::default(), b)),
            Err(NodetyError::CycleDetected)
        ));
    }

    // #[test]
    // fn test_add_remove_edge() {
    //     let mut system = NodeTypeSystem::new();
    //     let input_idx = system.add_node(sig("() -> (Integer)"));
    //     let output_idx = system.add_node(sig("(...Integer) -> ()"));
    //     system.add_edge(input_idx, output_idx, 0, 1);
    //     let edge_idx = system.add_edge(input_idx, output_idx, 0, 0);

    //     println!("{:?}", Dot::new(system.program()));

    //     system.remove_edge(edge_idx);
    //     let edge_idx = system.add_edge(input_idx, output_idx, 0, 0);

    //     println!("{:?}", Dot::new(system.program()));

    //     system.remove_edge(edge_idx);
    //     let edge_idx = system.add_edge(input_idx, output_idx, 0, 0);

    //     println!("{:?}", Dot::new(system.program()));
    // }
}
