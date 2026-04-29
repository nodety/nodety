use crate::common::graph;
use nodety::{NodeSignature, Nodety, demo_type::DemoType, inference::InferenceConfig};
use proptest::prelude::*;

mod common;

fn is_valid_sig(sig: &NodeSignature<DemoType>) -> bool {
    Nodety::new().add_node(sig.clone()).is_ok()
}

fn arb_graph_data() -> impl Strategy<Value = (Vec<NodeSignature<DemoType>>, Vec<(usize, usize, usize, usize)>)> {
    proptest::collection::vec(any::<NodeSignature<DemoType>>(), 2..12)
        .prop_map(|sigs| sigs.into_iter().filter(is_valid_sig).collect::<Vec<_>>())
        .prop_flat_map(|sigs| {
            let n = sigs.len().max(1);
            let edges_strategy = proptest::collection::vec((0..n, 0..n, 0..4usize, 0..4usize), 0..n * 2);
            (Just(sigs), edges_strategy)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn test_infer_and_validate_no_panic(
        (sigs, edges) in arb_graph_data()
    ) {
        prop_assume!(sigs.len() >= 2);
        let nodety = graph(sigs, edges);
        let scopes = nodety.infer(&InferenceConfig::default());
        let _errors = nodety.validate(&scopes);
    }
}
