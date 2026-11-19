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
        self.collect_candidates_in_order(
            source,
            own_scope,
            source_scope,
            infer_source_from_self,
            ignore_excluded,
            Variance::Covariant,
        )
        .into_iter()
        .collect()
    }

    /// [`Self::collect_candidates`] with the parameters in the order they are found, so that inference doesn't
    /// depend on hashing. `variance` is [Variance::Contravariant] if `self` flows into `source` instead.
    pub(crate) fn collect_candidates_in_order(
        &self,
        source: &Self,
        own_scope: &ScopePointer<T>,
        source_scope: &ScopePointer<T>,
        infer_source_from_self: bool,
        ignore_excluded: bool,
        variance: Variance,
    ) -> Vec<(GlobalParameterId<T>, Vec<Candidate<T>>)> {
        let mut candidates: Vec<(GlobalParameterId<T>, Vec<Candidate<T>>)> = Vec::new();

        self.traverse_parallel(
            source,
            own_scope,
            source_scope,
            infer_source_from_self,
            variance,
            &mut |own_type: &Self,
                  source_type: &Self,
                  own_traversal_scope: &ScopePointer<T>,
                  source_traversal_scope: &ScopePointer<T>,
                  variance: Variance| {
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
                if param_scope.lookup(&own_param.param_id).is_some_and(|(var, _)| !var.is_uninferred()) {
                    // Don't collect candidates for already inferred or rigid parameters.
                    return;
                }
                let global_id = GlobalParameterId { scope: param_scope, local_id: own_param.param_id };
                if own_scope.lookup_global(&global_id).is_none() {
                    // The var either
                    // - references a non existing type param
                    // - or it references a param that was defined within self.
                    return;
                }
                let candidate =
                    Candidate { t: source_type.clone(), scope: ScopePointer::clone(source_traversal_scope), variance };
                match candidates.iter_mut().find(|(id, _)| *id == global_id) {
                    Some((_, param_candidates)) => param_candidates.push(candidate),
                    None => candidates.push((global_id, vec![candidate])),
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

        let candidates = target.collect_candidates_in_order(
            &source,
            &target_scope,
            &source_scope,
            false,
            false,
            Variance::Covariant,
        );

        let expected = vec![(
            GlobalParameterId { scope: target_scope, local_id: "T".into() },
            vec![Candidate { t: expr("Integer"), scope: source_scope, variance: Variance::Covariant }],
        )];
        assert_eq!(expected, candidates);
    }

    /// Whether `pattern` (with the parameter `R`) collects a candidate for `R` from `other`.
    fn collects(pattern: &str, other: &str, variance: Variance) -> bool {
        let pattern_scope = ScopePointer::new(scope("<R>"));
        let root = ScopePointer::new_root();
        let candidates =
            expr(pattern).collect_candidates_in_order(&expr(other), &pattern_scope, &root, false, false, variance);
        !candidates.is_empty()
    }

    /// A concrete parameter of a constructor rules the other constructor out, so that none of its parameters is a
    /// candidate, if it isn't a supertype of the other one's where the other one flows in, and if it isn't a subtype
    /// of it where it flows out.
    #[test]
    fn a_constructor_is_ruled_out_in_the_direction_of_the_flow() {
        use Variance::{Contravariant, Covariant};
        let (wide, narrow, invariant) = ("{kind: Integer, row: R}", "{kind: 1, row: R}", "{kind: =Integer, row: R}");
        let (wide_other, narrow_other, foreign) =
            ("{kind: Integer, row: String}", "{kind: 1, row: String}", "{kind: String, row: String}");
        let closure = |input: &str| format!("({input}) -> ()");
        let cases = [
            (wide.to_string(), narrow_other.to_string(), Covariant, true),
            (narrow.to_string(), wide_other.to_string(), Covariant, false),
            (narrow.to_string(), wide_other.to_string(), Contravariant, true),
            (wide.to_string(), narrow_other.to_string(), Contravariant, false),
            // The inputs of a node signature flow the other way round.
            (closure(narrow), closure(wide_other), Covariant, true),
            (closure(wide), closure(narrow_other), Covariant, false),
            (closure(wide), closure(narrow_other), Contravariant, true),
            (closure(narrow), closure(wide_other), Contravariant, false),
            // An invariant parameter takes only itself, an unrelated one never fits.
            (invariant.to_string(), wide_other.to_string(), Covariant, true),
            (invariant.to_string(), narrow_other.to_string(), Covariant, false),
            (invariant.to_string(), wide_other.to_string(), Contravariant, true),
            (invariant.to_string(), narrow_other.to_string(), Contravariant, false),
            (wide.to_string(), foreign.to_string(), Covariant, false),
            (wide.to_string(), foreign.to_string(), Contravariant, false),
        ];
        for (pattern, other, variance, collected) in cases {
            assert_eq!(collects(&pattern, other.as_str(), variance), collected, "{pattern} / {other} {variance:?}");
        }
    }
}
