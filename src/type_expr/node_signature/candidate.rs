use crate::{
    scope::{ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{ScopePortal, ScopedTypeExpr},
};

/// A candidate for a type parameter.
#[derive(Debug, Clone)]
pub struct Candidate<T: Type> {
    /// The type of the candidate.
    pub t: ScopedTypeExpr<T>,
    /// The scope of the candidate.
    pub scope: ScopePointer<T>,
}

impl<T: Type> Candidate<T> {
    /// Picks the best candidate from the list, respecting bounds and preferring common supertypes.
    /// Returns `None` if the parameter bound is uninferred or no candidate satisfies it.
    pub fn pick_for_param(
        mut candidates: Vec<Candidate<T>>,
        type_param: &TypeParameter<T, ScopePortal<T>>,
        param_scope: &ScopePointer<T>,
    ) -> Option<Candidate<T>> {
        if let Some(bound) = &type_param.bound {
            if !bound.is_any(param_scope).unwrap_or(false) {
                // If the bound of the parameter is not yet fully inferred than it might narrow in the future.
                // If that happens, the candidates that are now chosen might violate the later bound.
                // To mitigate that, candidates can only be picked for a type param if the bound of the param
                // is guaranteed not to change.
                if bound.contains_uninferred(param_scope) {
                    return None;
                }

                // Discard all candidates that are not within the bounds of the parameter.
                // Or that could widen an in the future.
                candidates.retain(|c| {
                    bound
                        .supertype_of(&c.t, param_scope, &c.scope)
                        .is_supertype()
                        && !c.t.could_widen(&c.scope)
                });
            }
        }

        if candidates.is_empty() {
            return None;
        }
        if let Some(best_common_supertype) = Self::pick_best(&candidates) {
            Some(best_common_supertype.clone())
        } else {
            candidates.pop()
        }
    }

    fn pick_best(candidates: &[Candidate<T>]) -> Option<&Candidate<T>> {
        let mut common_supertypes = Vec::new();
        for (candidate_i, candidate) in candidates.iter().enumerate() {
            let mut supertype_of_all = true;
            for (test_i, test) in candidates.iter().enumerate() {
                if candidate_i == test_i {
                    continue;
                }
                if !candidate
                    .t
                    .supertype_of(&test.t, &candidate.scope, &test.scope)
                    .is_supertype()
                {
                    supertype_of_all = false;
                    break;
                }
            }
            if supertype_of_all {
                common_supertypes.push(candidate);
            }
        }
        // If there are more than one, apply rating
        common_supertypes.sort_by_key(|c| -rate_candidate(c));
        if common_supertypes.is_empty() {
            return None;
        }
        Some(common_supertypes.swap_remove(0))
    }
}

fn rate_candidate<T: Type>(candidate: &Candidate<T>) -> i8 {
    if candidate.t.is_never(&candidate.scope).unwrap_or(false) {
        return -1;
    };
    if candidate.t.contains_type_param() {
        1
    } else {
        0
    }
}
