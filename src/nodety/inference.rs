//! # Inference
//! This module contains the inference engine for a nodety type system.
//! It is used to infer the types of the nodes in a program.
use crate::{
    node_sorting::{SortDirection, sort_nodes_by_parent_depth},
    nodety::Nodety,
    scope::{GlobalParameterId, Scope, ScopePointer},
    r#type::Type,
    type_expr::{ScopedTypeExpr, TypeExpr, node_signature::candidate::Candidate},
};
use petgraph::graph::NodeIndex;
use petgraph::visit::IntoNodeReferences;
use petgraph::{
    Direction,
    visit::{EdgeRef, Topo},
};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug)]
pub enum FlowSourceLocation {
    Output(NodeIndex, usize),
    DefaultType(NodeIndex, usize),
}

#[derive(Debug)]
pub struct FlowTargetLocation {
    pub node_idx: NodeIndex,
    pub input_idx: usize,
}

impl FlowSourceLocation {
    pub fn node_idx(&self) -> NodeIndex {
        match self {
            FlowSourceLocation::Output(n, _) => *n,
            FlowSourceLocation::DefaultType(n, _) => *n,
        }
    }
}

/// A flow from one type into another.
/// Programs have two kinds of flows:
/// - Input to output
/// - and default type to input.
#[derive(Debug)]
pub struct Flow<T: Type> {
    pub source: ScopedTypeExpr<T>,
    pub source_scope: ScopePointer<T>,
    pub target: ScopedTypeExpr<T>,
    pub target_scope: ScopePointer<T>,
}

#[derive(Debug)]
pub struct FlowWithMetadata<T: Type> {
    pub flow: Flow<T>,
    pub source_location: FlowSourceLocation,
    pub target_location: FlowTargetLocation,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum InferenceDirection {
    Forward,
    Backward,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct InferenceStep {
    pub direction: InferenceDirection,
    pub allow_uninferred: bool,
    /// If true, target types will be used to infer the parameters of candidates.
    /// This is only relevant when working with generic functions (as ports).
    ///
    /// # Example:
    ///
    /// `|    <T>(T) -> (T) | ------- | (U) -> (Integer)    |`
    ///
    /// Here when `infer_candidates` is true, during candidate collection, the `T` will get inferred with Integer because it flows into that.
    /// If `infer_candidates` is false, the `T` will not get inferred during collection.
    ///
    /// For inferring nodes, inferring candidates is advantageous. But there are cases where this behavior is not desirable.
    /// For checking if a node signature is a supertype of another, the child must get inferred from the parent but not the other way around.
    /// Here `infer_candidates` will be set to false.
    ///
    /// See also: the `test_infer_during_candidate_collection` test in `tests/inference_tests.rs`.
    pub infer_candidates: bool,
    /// If true, type params with infer = false won't produce candidates, Otherwise all will emit candidates.
    pub ignore_excluded: bool,
}

impl InferenceStep {
    /// Recommended inference steps: covariant then contravariant, first strict then allowing uninferred.
    /// Strict backward runs before forward allow so that types (e.g. from sinks) propagate
    /// before identity-like flows infer params from uninferred sources.
    pub fn default_steps() -> Vec<InferenceStep> {
        vec![
            InferenceStep {
                direction: InferenceDirection::Forward,
                allow_uninferred: false,
                infer_candidates: true,
                ignore_excluded: false,
            },
            InferenceStep {
                direction: InferenceDirection::Backward,
                allow_uninferred: false,
                infer_candidates: true,
                ignore_excluded: false,
            },
            InferenceStep {
                direction: InferenceDirection::Forward,
                allow_uninferred: true,
                infer_candidates: true,
                ignore_excluded: false,
            },
            InferenceStep {
                direction: InferenceDirection::Backward,
                allow_uninferred: true,
                infer_candidates: true,
                ignore_excluded: false,
            },
        ]
    }
}

pub type Scopes<T> = BTreeMap<NodeIndex, ScopePointer<T>>;

#[derive(Debug, Clone)]
pub struct InferenceConfig<T: Type> {
    /// The steps to perform the inference. (InferenceStep::default_steps() is usually the best choice)
    pub steps: Vec<InferenceStep>,
    /// If Some, only these parameters will get inferred. All others will stay untouched.
    pub restrictions: Option<HashSet<GlobalParameterId<T>>>,
    /// If some, inference will stop as soon as all of these parameters are inferred.
    /// Others might still get inferred first in order to get to these.
    pub stop_after: Option<HashSet<GlobalParameterId<T>>>,
}

impl<T: Type> Default for InferenceConfig<T> {
    fn default() -> Self {
        Self { steps: InferenceStep::default_steps(), restrictions: None, stop_after: None }
    }
}

pub fn infer<T: Type>(mut flows: Vec<Flow<T>>, config: &InferenceConfig<T>) {
    let mut stop_after = config.stop_after.clone();
    for step in &config.steps {
        // Optimization:
        // All flows that don't contain any uninferred parameters won't produce
        // any candidates and can thus be removed in the name of efficiency.
        flows.retain(|flow| {
            flow.source.contains_uninferred(&flow.source_scope) || flow.target.contains_uninferred(&flow.target_scope)
        });
        loop {
            // Candidate collection
            let mut all_candidates = HashMap::new();
            for flow in &flows {
                let candidates = if step.direction == InferenceDirection::Forward {
                    flow.target.collect_candidates(
                        &flow.source,
                        &flow.target_scope,
                        &flow.source_scope,
                        step.infer_candidates,
                        step.ignore_excluded,
                    )
                } else {
                    flow.source.collect_candidates(
                        &flow.target,
                        &flow.source_scope,
                        &flow.target_scope,
                        step.infer_candidates,
                        step.ignore_excluded,
                    )
                };
                for (global_param_id, candidate) in candidates {
                    if let Some(restrictions) = &config.restrictions {
                        if !restrictions.contains(&global_param_id) {
                            continue;
                        }
                    }
                    all_candidates.entry(global_param_id).or_insert_with(Vec::new).extend(candidate);
                }
            }
            if !step.allow_uninferred {
                for (_gid, candidates) in all_candidates.iter_mut() {
                    candidates.retain(|candidate| !candidate.t.contains_uninferred(&candidate.scope));
                }
            }
            let mut progress = false;

            // Candidate picking
            for (global_id, mut candidates) in all_candidates {
                let Some((registered_param, param_scope)) = global_id.scope.lookup(&global_id.local_id) else {
                    continue;
                };
                if registered_param.is_inferred() {
                    continue;
                }

                candidates
                    .retain(|candidate| !candidate.t.references(&HashSet::from([global_id.clone()]), &candidate.scope));
                let Some(picked_candidate) =
                    Candidate::pick_for_param(candidates, registered_param.parameter(), param_scope)
                else {
                    continue;
                };
                if param_scope.infer(&global_id.local_id, picked_candidate.t.clone(), picked_candidate.scope).is_ok() {
                    // println!("inferred {:?} = {:#?}", global_id.local_id, picked_candidate.t);
                    if let Some(sa) = &mut stop_after {
                        sa.remove(&global_id);
                        if sa.is_empty() {
                            return;
                        }
                    }
                    progress = true;
                }
            }
            if !progress {
                // The current stage didn't produce any progress.
                // Move on to the next step.
                break;
            }
        }
    }
}

impl<T: Type> Nodety<T> {
    /// # Panics
    /// if the node is not found in the program.
    pub(super) fn build_node_scope(&self, node_idx: NodeIndex) -> ScopePointer<T> {
        let mut chain = vec![node_idx];
        let mut current = node_idx;
        while let Some(parent) = self.program[current].parent {
            chain.push(parent);
            current = parent;
        }

        let mut scope = ScopePointer::new(Scope::new_root());
        for &idx in chain.iter().rev() {
            let mut child_scope = Scope::new_child(&scope);
            for (local_param_id, param) in &self.program[idx].signature.parameters {
                child_scope.define(*local_param_id, param.clone().into_scoped());
            }
            scope = ScopePointer::new(child_scope);
        }
        scope
    }

    // @todo: make this performant
    pub fn build_scopes(&self) -> BTreeMap<NodeIndex, ScopePointer<T>> {
        let mut scopes: BTreeMap<NodeIndex, ScopePointer<T>> = BTreeMap::new();

        let mut node_references = self.program.node_references().collect::<Vec<_>>();

        sort_nodes_by_parent_depth(
            &mut node_references,
            SortDirection::Asc,
            |(id, _node)| *id,
            |(_, node)| node.parent,
        )
        .expect("expected no cycles in parent relations");

        for (node_idx, node) in node_references {
            if scopes.contains_key(&node_idx) {
                continue;
            }
            let mut scope = if let Some(parent) = &node.parent {
                let Some(parent_scope) = scopes.get(parent) else {
                    continue;
                };
                Scope::new_child(parent_scope)
            } else {
                Scope::new_root()
            };
            for (local_param_id, param) in &node.signature.parameters {
                scope.define(*local_param_id, param.clone().into_scoped());
            }
            scopes.insert(node_idx, ScopePointer::new(scope));
        }
        scopes
    }

    /// Infers the type parameters for all nodes in the program.
    ///
    /// # Parameters
    /// - `steps`: The steps to perform the inference. (InferenceStep::default_steps() is usually the best choice)
    pub fn infer(&self, config: &InferenceConfig<T>) -> Scopes<T> {
        let scopes = self.build_scopes();
        let flows = self.collect_flows(&scopes);
        infer(flows.into_iter().map(|flow| flow.flow).collect(), config);
        scopes
    }

    pub fn collect_flows(&self, scopes: &Scopes<T>) -> Vec<FlowWithMetadata<T>> {
        let mut flows: Vec<FlowWithMetadata<T>> = Vec::with_capacity(self.program.edge_count());
        // contains node indices with their input indices.
        let mut populated_inputs: HashSet<(NodeIndex, usize)> = HashSet::new();

        // Collect flows in topological order so that root nodes get inferred first
        let mut topo = Topo::new(&self.program);
        while let Some(node_idx) = topo.next(&self.program) {
            // dbg!(&self.program[node_idx].default_input_types);
            for edge in self.program.edges_directed(node_idx, Direction::Outgoing) {
                populated_inputs.insert((edge.target(), edge.weight().target_port));

                let source_node = &self.program[edge.source()];
                let target_node = &self.program[edge.target()];

                let TypeExpr::PortTypes(source_ports) = &source_node.signature.outputs else {
                    continue;
                };
                let TypeExpr::PortTypes(target_ports) = &target_node.signature.inputs else {
                    continue;
                };

                let Some(source_port) = source_ports.get_port_type(edge.weight().source_port) else {
                    continue;
                };
                let Some(target_port) = target_ports.get_port_type(edge.weight().target_port) else {
                    continue;
                };

                let Some(source_scope) = scopes.get(&edge.source()) else {
                    continue;
                };
                let Some(target_scope) = scopes.get(&edge.target()) else {
                    continue;
                };

                flows.push(FlowWithMetadata {
                    flow: Flow {
                        source: source_port.clone().into_scoped(),
                        target: target_port.clone().into_scoped(),
                        source_scope: ScopePointer::clone(source_scope),
                        target_scope: ScopePointer::clone(target_scope),
                    },
                    source_location: FlowSourceLocation::Output(edge.source(), edge.weight().source_port),
                    target_location: FlowTargetLocation {
                        node_idx: edge.target(),
                        input_idx: edge.weight().target_port,
                    },
                });
            }
        }

        // Apply default types for inputs that don't have an edge.
        let mut topo = Topo::new(&self.program);
        while let Some(node_idx) = topo.next(&self.program) {
            let node = &self.program[node_idx];
            for (input_idx, default_type) in &node.signature.default_input_types {
                if populated_inputs.contains(&(node_idx, *input_idx)) {
                    continue;
                }
                let TypeExpr::PortTypes(ports) = &node.signature.inputs else {
                    continue;
                };
                let Some(port) = ports.get_port_type(*input_idx) else {
                    continue;
                };
                let Some(scope) = scopes.get(&node_idx) else {
                    continue;
                };
                flows.push(FlowWithMetadata {
                    flow: Flow {
                        source: default_type.clone().into_scoped(),
                        target: port.clone().into_scoped(),
                        source_scope: ScopePointer::clone(scope),
                        target_scope: ScopePointer::clone(scope),
                    },
                    source_location: FlowSourceLocation::DefaultType(node_idx, *input_idx),
                    target_location: FlowTargetLocation { node_idx, input_idx: *input_idx },
                });
            }
        }
        flows
    }
}
