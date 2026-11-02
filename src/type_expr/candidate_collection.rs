use crate::{
    scope::{GlobalParameterId, ScopePointer},
    r#type::Type,
    type_expr::{ScopedTypeExpr, node_signature::candidate::Candidate, traversal::Variance},
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
        self.collect_candidates_in_order(source, own_scope, source_scope, infer_source_from_self, ignore_excluded)
            .into_iter()
            .collect()
    }

    /// [`Self::collect_candidates`] with the parameters in the order they are found, so that inference doesn't
    /// depend on hashing.
    pub(crate) fn collect_candidates_in_order(
        &self,
        source: &Self,
        own_scope: &ScopePointer<T>,
        source_scope: &ScopePointer<T>,
        infer_source_from_self: bool,
        ignore_excluded: bool,
    ) -> Vec<(GlobalParameterId<T>, Vec<Candidate<T>>)> {
        let mut candidates: Vec<(GlobalParameterId<T>, Vec<Candidate<T>>)> = Vec::new();
        let mut emit = |param_type: &Self,
                        param_traversal_scope: &ScopePointer<T>,
                        root_scope: &ScopePointer<T>,
                        candidate: Candidate<T>| {
            // if the param has infer == false, no candidates get emitted from here.
            let Some(param) = param_type.as_param() else {
                return;
            };
            if !param.infer && !ignore_excluded {
                return;
            }
            let Some(param_scope) = param_traversal_scope.lookup_scope(&param.param_id) else {
                return;
            };
            if param_scope.is_inferred(&param.param_id) {
                // Don't collect candidates for already inferred parameters.
                return;
            }
            let global_id = GlobalParameterId { scope: param_scope, local_id: param.param_id };
            if root_scope.lookup_global(&global_id).is_none() {
                // The var either
                // - references a non existing type param
                // - or it references a param that was defined within the traversed expression.
                return;
            }
            match candidates.iter_mut().find(|(id, _)| *id == global_id) {
                Some((_, param_candidates)) => param_candidates.push(candidate),
                None => candidates.push((global_id, vec![candidate])),
            }
        };

        self.traverse_parallel(
            source,
            own_scope,
            source_scope,
            infer_source_from_self,
            Variance::Covariant,
            &mut |own_type: &Self,
                  source_type: &Self,
                  own_traversal_scope: &ScopePointer<T>,
                  source_traversal_scope: &ScopePointer<T>,
                  variance: Variance| {
                let candidate =
                    Candidate { t: source_type.clone(), scope: ScopePointer::clone(source_traversal_scope) };
                emit(own_type, own_traversal_scope, own_scope, candidate);
                // An invariant position binds parameters on both sides, whatever the direction of the flow.
                if variance == Variance::Invariant {
                    let candidate = Candidate { t: own_type.clone(), scope: ScopePointer::clone(own_traversal_scope) };
                    emit(source_type, source_traversal_scope, source_scope, candidate);
                }
            },
        );
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::parse::{expr, scope};

    #[test]
    fn test_collect_candidates() {
        let source = expr("Array<Integer>");
        let target = expr("Array<T>");

        let target_scope = ScopePointer::new(scope("<T>"));
        let source_scope = ScopePointer::new_root();

        let candidates = target.collect_candidates_in_order(&source, &target_scope, &source_scope, false, false);

        let expected = vec![(
            GlobalParameterId { scope: target_scope, local_id: "T".into() },
            vec![Candidate { t: expr("Integer"), scope: source_scope }],
        )];
        assert_eq!(expected, candidates);
    }
}
