use core::fmt;
use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Display},
    hash::Hash,
};

#[derive(PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug)]
pub enum NodeSortingError<ID> {
    CyclicParentRelation { cycle_node_ids: Vec<ID> },
}

impl<ID: Debug + Display> std::error::Error for NodeSortingError<ID> {}

impl<ID: Debug + Display> Display for NodeSortingError<ID> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Sorts nodes by the depth of their parents.
///
/// Nodes that have a parent which is not found in nodes, get treated as if they didn't have a parent.
///
/// ## Example:
///
/// A deleted node might be a child of a node that doesn't get deleted. Then the parent is not found in the list but that is ok.
pub fn sort_nodes_by_parent_depth<T, ID, IDExtractor, ParentExtractor>(
    nodes: &mut Vec<T>,
    direction: SortDirection,
    extract_id: IDExtractor,
    extract_parent: ParentExtractor,
) -> Result<(), NodeSortingError<ID>>
where
    IDExtractor: Fn(&T) -> ID,
    ParentExtractor: Fn(&T) -> Option<ID>,
    ID: Eq + Hash + Clone,
{
    // Create a map from id to index for quick lookup
    let id_to_index: HashMap<ID, usize> = nodes.iter().enumerate().map(|(idx, node)| (extract_id(node), idx)).collect();

    // Calculate depth for each node (also validates no cycles)
    let mut depths = Vec::with_capacity(nodes.len());
    for i in 0..nodes.len() {
        let depth = calculate_depth::<T, ID, IDExtractor, ParentExtractor>(
            i,
            nodes,
            &id_to_index,
            &extract_id,
            &extract_parent,
        )?;
        depths.push(depth);
    }

    // Pair each node with its depth, then sort
    let mut pairs: Vec<(i64, T)> = nodes.drain(..).enumerate().map(|(i, node)| (depths[i] as i64, node)).collect();

    pairs.sort_by_key(|(depth, _)| match direction {
        SortDirection::Asc => *depth,
        SortDirection::Desc => -*depth,
    });

    // Put the sorted nodes back
    nodes.extend(pairs.into_iter().map(|(_, node)| node));

    Ok(())
}

fn calculate_depth<T, ID, IDExtractor, ParentExtractor>(
    start_idx: usize,
    nodes: &[T],
    id_to_index: &HashMap<ID, usize>,
    extract_id: &IDExtractor,
    extract_parent: &ParentExtractor,
) -> Result<usize, NodeSortingError<ID>>
where
    IDExtractor: Fn(&T) -> ID,
    ParentExtractor: Fn(&T) -> Option<ID>,
    ID: Eq + Hash + Clone,
{
    let mut visited = Vec::new();
    let mut visited_set = HashSet::new();
    let mut current_idx = start_idx;
    let mut depth = 0;

    loop {
        let current_id = extract_id(&nodes[current_idx]);

        // Check for cycle
        if visited_set.contains(&current_idx) {
            // Collect all node IDs in the cycle
            visited.push(current_id);
            return Err(NodeSortingError::CyclicParentRelation { cycle_node_ids: visited });
        }

        visited.push(current_id.clone());
        visited_set.insert(current_idx);

        // Check if we've reached a root node (no parent)
        match extract_parent(&nodes[current_idx]) {
            None => return Ok(depth),
            Some(parent_id) => {
                // Find the parent node
                match id_to_index.get(&parent_id) {
                    None => return Ok(depth), // Ignore this parent
                    Some(&parent_idx) => {
                        current_idx = parent_idx;
                        depth += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DemoNode {
        id: usize,
        parent_node: Option<usize>,
    }

    impl DemoNode {
        fn build_parent_relation(id: usize, parent: Option<usize>) -> Self {
            Self { id, parent_node: parent }
        }
    }

    #[test]
    fn test_sort_no_parents() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(3, None),
            DemoNode::build_parent_relation(1, None),
            DemoNode::build_parent_relation(2, None),
        ];

        sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node).unwrap();

        // All nodes have depth 0, order should be preserved within same depth
        assert_eq!(nodes.len(), 3);
        for node in &nodes {
            assert_eq!(node.parent_node, None);
        }
    }

    #[test]
    fn test_sort_simple_hierarchy() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(3, Some(2)), // depth 2
            DemoNode::build_parent_relation(1, None),    // depth 0 (root)
            DemoNode::build_parent_relation(2, Some(1)), // depth 1
        ];

        sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node).unwrap();

        assert_eq!(nodes[0].id, 1); // root first
        assert_eq!(nodes[1].id, 2); // child of root
        assert_eq!(nodes[2].id, 3); // grandchild last
    }

    #[test]
    fn test_sort_simple_hierarchy_desc() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(3, Some(2)), // depth 2
            DemoNode::build_parent_relation(1, None),    // depth 0 (root)
            DemoNode::build_parent_relation(2, Some(1)), // depth 1
        ];

        sort_nodes_by_parent_depth(&mut nodes, SortDirection::Desc, |n| n.id, |n| n.parent_node).unwrap();

        assert_eq!(nodes[0].id, 3);
        assert_eq!(nodes[1].id, 2);
        assert_eq!(nodes[2].id, 1);
    }

    #[test]
    fn test_sort_multiple_roots() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(4, Some(3)), // depth 1
            DemoNode::build_parent_relation(2, Some(1)), // depth 1
            DemoNode::build_parent_relation(1, None),    // depth 0
            DemoNode::build_parent_relation(3, None),    // depth 0
        ];

        let result = sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node);
        assert!(result.is_ok());

        // First two should be roots
        assert_eq!(nodes[0].parent_node, None);
        assert_eq!(nodes[1].parent_node, None);

        // Last two should have parents
        assert!(nodes[2].parent_node.is_some());
        assert!(nodes[3].parent_node.is_some());
    }

    #[test]
    fn test_cyclic_relation_self_reference() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(1, Some(1)), // self-reference
        ];

        let result = sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node);
        assert!(result.is_err());

        match result {
            Err(NodeSortingError::CyclicParentRelation { cycle_node_ids }) => {
                assert!(cycle_node_ids.contains(&1));
            }
            _ => panic!("Expected CyclicParentRelation error"),
        }
    }

    #[test]
    fn test_cyclic_relation_two_nodes() {
        let mut nodes = vec![DemoNode::build_parent_relation(1, Some(2)), DemoNode::build_parent_relation(2, Some(1))];

        let result = sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node);
        assert!(result.is_err());

        match result {
            Err(NodeSortingError::CyclicParentRelation { cycle_node_ids }) => {
                assert!(cycle_node_ids.contains(&1));
                assert!(cycle_node_ids.contains(&2));
            }
            _ => panic!("Expected CyclicParentRelation error"),
        }
    }

    #[test]
    fn test_cyclic_relation_three_nodes() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(1, Some(2)),
            DemoNode::build_parent_relation(2, Some(3)),
            DemoNode::build_parent_relation(3, Some(1)),
        ];

        let result = sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node);
        assert!(result.is_err());

        match result {
            Err(NodeSortingError::CyclicParentRelation { cycle_node_ids }) => {
                assert_eq!(cycle_node_ids.len(), 4); // 1->2->3->1
                assert!(cycle_node_ids.contains(&1));
                assert!(cycle_node_ids.contains(&2));
                assert!(cycle_node_ids.contains(&3));
            }
            _ => panic!("Expected CyclicParentRelation error"),
        }
    }

    #[test]
    fn test_missing_parent() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(1, Some(99)), // parent 99 doesn't exist
        ];

        sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node).unwrap();
    }

    #[test]
    fn test_complex_hierarchy() {
        let mut nodes = vec![
            DemoNode::build_parent_relation(7, Some(5)), // depth 3
            DemoNode::build_parent_relation(1, None),    // depth 0
            DemoNode::build_parent_relation(5, Some(3)), // depth 2
            DemoNode::build_parent_relation(3, Some(1)), // depth 1
            DemoNode::build_parent_relation(2, None),    // depth 0
            DemoNode::build_parent_relation(4, Some(2)), // depth 1
            DemoNode::build_parent_relation(6, Some(4)), // depth 2
        ];

        sort_nodes_by_parent_depth(&mut nodes, SortDirection::Asc, |n| n.id, |n| n.parent_node).unwrap();

        // Verify sorting by depth
        let depths: Vec<usize> = nodes
            .iter()
            .map(|node| {
                let mut depth = 0;
                let mut current_parent = node.parent_node;
                while let Some(parent_id) = current_parent {
                    depth += 1;
                    current_parent = nodes.iter().find(|n| n.id == parent_id).and_then(|n| n.parent_node);
                }
                depth
            })
            .collect();

        // Check that depths are in ascending order
        for i in 1..depths.len() {
            assert!(depths[i] >= depths[i - 1]);
        }
    }
}
