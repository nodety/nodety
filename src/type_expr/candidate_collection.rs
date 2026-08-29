use crate::{
    scope::{GlobalParameterId, ScopePointer},
    r#type::Type,
    type_expr::{ScopedTypeExpr, node_signature::candidate::Candidate},
};
use std::collections::HashMap;

impl<T: Type> ScopedTypeExpr<T> {
    /// # Parameters
    /// - `ignore_excluded`: if false, type params with infer = false won't produce candidates, Otherwise all will emit candidates.
    pub fn collect_candidates(
        &self,
        source: &Self,
        own_scope: &ScopePointer<T>,
        source_scope: &ScopePointer<T>,
        infer_source_from_self: bool,
        ignore_excluded: bool,
    ) -> HashMap<GlobalParameterId<T>, Vec<Candidate<T>>> {
        let mut candidates = HashMap::new();

        self.traverse_parallel(
            source,
            own_scope,
            source_scope,
            infer_source_from_self,
            &mut |own_type: &Self,
                  source_type: &Self,
                  own_traversal_scope: &ScopePointer<T>,
                  source_traversal_scope: &ScopePointer<T>| {
                // if the own_type has infer == false, no candidates get emitted from here.
                let Some(own_param) = own_type.as_param() else {
                    return;
                };
                if !own_param.infer && !ignore_excluded {
                    return;
                }
                let Some(param_scope) = own_traversal_scope.lookup_scope(&own_param.param_id) else {
                    return;
                };
                if param_scope.is_inferred(&own_param.param_id) {
                    // Don't collect candidates for already inferred parameters.
                    return;
                }
                let global_id = GlobalParameterId { scope: param_scope, local_id: own_param.param_id };
                if own_scope.lookup_global(&global_id).is_none() {
                    // The var either
                    // - references a non existing type param
                    // - or it references a param that was defined within self.
                    return;
                }
                candidates
                    .entry(global_id)
                    .or_insert(Vec::new())
                    .push(Candidate { t: source_type.clone(), scope: ScopePointer::clone(source_traversal_scope) });
            },
        );
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::parse::{expr, scope};
    use maplit::hashmap;

    #[test]
    fn test_collect_candidates() {
        let source = expr("Array<Integer>");
        let target = expr("Array<T>");

        let target_scope = ScopePointer::new(scope("<T>"));
        let source_scope = ScopePointer::new_root();

        let candidates = target.collect_candidates(&source, &target_scope, &source_scope, false, false);

        let expected = hashmap! { GlobalParameterId { scope: target_scope, local_id: "T".into() } => vec![Candidate { t: expr("Integer"), scope: source_scope }]};
        assert_eq!(expected, candidates);
    }
}
