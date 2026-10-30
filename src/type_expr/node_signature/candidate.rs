use crate::{
    scope::{ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{ConstructorParams, ScopedTypeExpr, ScopedTypeRef, TypeExpr},
};

/// A candidate for a type parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate<T: Type> {
    /// The type of the candidate.
    pub t: ScopedTypeExpr<T>,
    /// The scope of the candidate.
    pub scope: ScopePointer<T>,
}

impl<T: Type> Candidate<T> {
    /// Removes candidates that duplicate a single arm of a top-level union candidate.
    ///
    /// During union traversal, candidate collection emits both the whole union (from the initial
    /// walker call) and each branch separately. Branch-only candidates can survive the uninferred
    /// filter before the whole union becomes concrete; dropping them avoids premature picks.
    ///
    /// # Example:
    /// candidates: `[T | Unit, T, Unit]`
    /// After dedupe: `[T | Unit]`
    pub fn drop_union_branch_duplicates(candidates: &mut Vec<Candidate<T>>) {
        let union_arms: Vec<_> = candidates
            .iter()
            .filter(|candidate| matches!(candidate.t, TypeExpr::Union(_, _)))
            .flat_map(|candidate| collect_union_arms(&candidate.t, &candidate.scope))
            .collect();

        if union_arms.is_empty() {
            return;
        }

        candidates.retain(|candidate| {
            if matches!(candidate.t, TypeExpr::Union(_, _)) {
                return true;
            }
            let normalized = candidate.t.normalize(&candidate.scope);
            !union_arms.iter().any(|arm| arm == &normalized)
        });
    }

    /// Picks the best candidate from the list, respecting bounds.
    /// Candidates flowing into the param are lower bounds and pick the common supertype.
    /// Candidates the param flows into are `upper_bounds` and pick the common subtype.
    /// Returns `None` if the parameter bound is uninferred or no candidate satisfies it.
    pub fn pick_for_param(
        mut candidates: Vec<Candidate<T>>,
        type_param: &TypeParameter<T, ScopedTypeRef<T>>,
        param_scope: &ScopePointer<T>,
        upper_bounds: bool,
    ) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        if let Some(bound) = &type_param.bound
            && !bound.is_any(param_scope).unwrap_or(false)
        {
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
                bound.supertype_of(&c.t, param_scope, &c.scope).is_supertype() && !c.t.could_widen(&c.scope)
            });
        }

        if candidates.is_empty() {
            return None;
        }
        if upper_bounds {
            return Self::pick_meet(candidates);
        }
        if let Some(join) = Self::join_constructors(&candidates)
            && type_param
                .bound
                .as_ref()
                .is_none_or(|bound| bound.supertype_of(&join, param_scope, &ScopePointer::new_root()).is_supertype())
        {
            return Some((join, ScopePointer::new_root()));
        }
        if let Some(best_common_supertype) = Self::pick_best(&candidates) {
            return Some((best_common_supertype.t.clone(), best_common_supertype.scope.clone()));
        }
        if let Some(common_supertype) = Self::common_supertype(&candidates)
            && type_param.bound.as_ref().is_none_or(|bound| {
                bound.supertype_of(&common_supertype, param_scope, &ScopePointer::new_root()).is_supertype()
            })
        {
            return Some((common_supertype, ScopePointer::new_root()));
        }
        Some((
            TypeExpr::from_unions(
                candidates
                    .into_iter()
                    .map(|c| TypeExpr::Ref(ScopedTypeRef::ScopedExpr { expr: Box::new(c.t), scope: c.scope })),
            ),
            ScopePointer::new_root(),
        ))
    }

    /// The param must be a subtype of all upper bounds.
    /// Picks the candidate that is a subtype of all others or the intersection of all candidates if it is not never.
    fn pick_meet(candidates: Vec<Candidate<T>>) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        let common_subtype = candidates.iter().find(|candidate| {
            candidates
                .iter()
                .all(|other| other.t.supertype_of(&candidate.t, &other.scope, &candidate.scope).is_supertype())
        });
        if let Some(common_subtype) = common_subtype {
            return Some((common_subtype.t.clone(), common_subtype.scope.clone()));
        }
        let mut candidates = candidates.into_iter();
        let first = candidates.next()?;
        let (meet, meet_scope) = candidates.try_fold((first.t, first.scope), |(meet, meet_scope), candidate| {
            TypeExpr::intersection(&meet, &candidate.t, &meet_scope, &candidate.scope)
        })?;
        if meet.is_never(&meet_scope).unwrap_or(true) {
            return None;
        }
        Some((meet, meet_scope))
    }

    /// The [Type::common_supertype] of all candidates, if all of them are plain types and the type system has one.
    fn common_supertype(candidates: &[Candidate<T>]) -> Option<ScopedTypeExpr<T>> {
        let mut types = candidates.iter().map(|candidate| match candidate.t.normalize(&candidate.scope) {
            TypeExpr::Type(t) => Some(t),
            _ => None,
        });
        let first = types.next()??;
        types
            .try_fold(first, |common, t| {
                let t = t?;
                if common.supertype_of(&t) {
                    Some(common)
                } else if t.supertype_of(&common) {
                    Some(t)
                } else {
                    common.common_supertype(&t)
                }
            })
            .map(TypeExpr::Type)
    }

    /// The field-by-field join of several constructor candidates, see [Type::optional_field_type]. `None` if the type
    /// system has no optional field type, there are fewer than two candidates or one is not a constructor.
    fn join_constructors(candidates: &[Candidate<T>]) -> Option<ScopedTypeExpr<T>> {
        let optional = TypeExpr::Type(T::optional_field_type()?);
        if candidates.len() < 2 {
            return None;
        }
        let mut types = candidates.iter().map(|candidate| candidate.t.normalize(&candidate.scope));
        let first = types.next()?;
        types.try_fold(first, |joined, t| join_constructor(&joined, &t, &optional))
    }

    fn pick_best(candidates: &[Candidate<T>]) -> Option<&Candidate<T>> {
        let mut common_supertypes = Vec::new();
        for (candidate_i, candidate) in candidates.iter().enumerate() {
            let mut supertype_of_all = true;
            for (test_i, test) in candidates.iter().enumerate() {
                if candidate_i == test_i {
                    continue;
                }
                if !candidate.t.supertype_of(&test.t, &candidate.scope, &test.scope).is_supertype() {
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

/// `a` and `b` joined field by field if both are constructors of the same type, see [Type::optional_field_type].
fn join_constructor<T: Type>(
    a: &ScopedTypeExpr<T>,
    b: &ScopedTypeExpr<T>,
    optional: &ScopedTypeExpr<T>,
) -> Option<ScopedTypeExpr<T>> {
    let (
        TypeExpr::Constructor { inner: inner_a, parameters: fields_a },
        TypeExpr::Constructor { inner: inner_b, parameters: fields_b },
    ) = (a, b)
    else {
        return None;
    };
    if inner_a != inner_b {
        return None;
    }
    let with_optional =
        |field: &ScopedTypeExpr<T>| field.clone().union_with(optional.clone()).normalize(&ScopePointer::new_root());
    let mut fields = ConstructorParams::new();
    for (name, field_a) in fields_a.iter() {
        let joined = match fields_b.get(name) {
            Some(field_b) => join_field(field_a, field_b, optional),
            None => with_optional(field_a),
        };
        fields.insert(name.clone(), joined);
    }
    for (name, field_b) in fields_b.iter().filter(|(name, _)| !fields_a.contains_key(name)) {
        fields.insert(name.clone(), with_optional(field_b));
    }
    Some(TypeExpr::Constructor { inner: inner_a.clone(), parameters: fields })
}

/// The common supertype of two field types: the joined constructors, the wider one, the type system's
/// [Type::common_supertype] or else their union.
fn join_field<T: Type>(
    a: &ScopedTypeExpr<T>,
    b: &ScopedTypeExpr<T>,
    optional: &ScopedTypeExpr<T>,
) -> ScopedTypeExpr<T> {
    let root = ScopePointer::new_root();
    if a == b {
        return a.clone();
    }
    if let Some(joined) = join_constructor(a, b, optional) {
        return joined;
    }
    if a.supertype_of(b, &root, &root).is_supertype() {
        return a.clone();
    }
    if b.supertype_of(a, &root, &root).is_supertype() {
        return b.clone();
    }
    if let (TypeExpr::Type(type_a), TypeExpr::Type(type_b)) = (a, b)
        && let Some(common) = type_a.common_supertype(type_b)
    {
        return TypeExpr::Type(common);
    }
    a.clone().union_with(b.clone()).normalize(&root)
}

fn collect_union_arms<T: Type>(expr: &ScopedTypeExpr<T>, scope: &ScopePointer<T>) -> Vec<ScopedTypeExpr<T>> {
    match expr {
        TypeExpr::Union(a, b) => {
            let mut arms = collect_union_arms(a, scope);
            arms.extend(collect_union_arms(b, scope));
            arms
        }
        other => vec![other.normalize(scope)],
    }
}

fn rate_candidate<T: Type>(candidate: &Candidate<T>) -> i8 {
    if candidate.t.is_never(&candidate.scope).unwrap_or(false) {
        return -1;
    };
    if candidate.t.contains_type_param() { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::parse::{expr, scope};
    use crate::scope::ScopePointer;

    fn candidate(t: &str, scope: ScopePointer<crate::demo_type::DemoType>) -> Candidate<crate::demo_type::DemoType> {
        Candidate { t: expr(t), scope }
    }

    #[test]
    fn drop_union_branch_duplicates_removes_arms() {
        let s = ScopePointer::new(scope("<T>"));
        let mut candidates = vec![
            candidate("T | Unit", ScopePointer::clone(&s)),
            candidate("T", ScopePointer::clone(&s)),
            candidate("Unit", ScopePointer::clone(&s)),
        ];

        Candidate::drop_union_branch_duplicates(&mut candidates);

        assert_eq!(vec![candidate("T | Unit", s)], candidates);
    }

    #[test]
    fn drop_union_branch_duplicates_keeps_unrelated_candidates() {
        let s = ScopePointer::new(scope("<T>"));
        let mut candidates =
            vec![candidate("Integer", ScopePointer::clone(&s)), candidate("String", ScopePointer::clone(&s))];

        Candidate::drop_union_branch_duplicates(&mut candidates);

        assert_eq!(vec![candidate("Integer", ScopePointer::clone(&s)), candidate("String", s)], candidates);
    }
}
