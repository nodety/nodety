use crate::{
    Type, TypeExpr,
    scope::ScopePointer,
    type_expr::{ScopedTypeRef, TypeRef},
};

impl<T: Type, R: TypeRef> TypeExpr<T, R> {
    /// Traverses this expression for all "top level" unions.
    /// All non union expressions are considered leafs that are given to walker but not walked any further.
    pub fn traverse_union_non_context_sensitive<'a>(&'a self, walker: &mut impl FnMut(&'a Self)) {
        match self {
            Self::Union(a, b) => {
                a.traverse_union_non_context_sensitive(walker);
                b.traverse_union_non_context_sensitive(walker);
            }
            _ => walker(self),
        }
    }
}

// Traversal functions cannot be generic over R because the visitor functions have to always accept
// ScopedTypeExprs for walking inferred types.
impl<T: Type> TypeExpr<T, ScopedTypeRef<T>> {
    /// Calls walker for all types that are a "top level" union in self. Check out TypeExpr::traverse_mut for more infos
    /// Always visits
    pub fn traverse_union_mut(
        &mut self,
        scope: &ScopePointer<T>,
        walker: &mut impl FnMut(&mut Self, &ScopePointer<T>),
    ) {
        self.traverse_mut(
            scope,
            &mut |type_expr, scope, is_top_level_union| {
                if is_top_level_union {
                    walker(type_expr, scope);
                }
            },
            true,
        );
    }

    /// Calls walker for all types that are a "top level" union in self. Check out (TypeExpr::traverse_mut)[Self::traverse_mut] for more details.
    /// Always visits at least one type expr.
    pub fn traverse_union(&self, scope: &ScopePointer<T>, walker: &mut impl FnMut(&Self, &ScopePointer<T>)) {
        self.traverse(
            scope,
            &mut |type_expr, scope, is_top_level_union| {
                if !is_top_level_union {
                    return;
                }
                if let Some(param) = type_expr.as_param()
                    && scope.is_inferred(&param.param_id)
                {
                    // If the param is inferred, self.traverse will look it up and call this walker again.
                    return;
                }
                walker(type_expr, scope);
            },
            true,
        );
    }

    /// Mutable version of `traverse` with the exception, that the inferred
    /// types of type parameters don't get visited because they are immutable.
    /// # Walker
    /// (type_expr, scope, is_top_level_union) -> ()
    /// # Unions
    /// is_top_level_union is true if the type_expr itself is not a union but a type that
    /// is part of the started types union (One type is always a union).
    ///
    /// for T | null | keyof T
    /// - T | null | keyof T: false
    /// - T: true
    /// - null: true
    /// - keyof T: true
    /// - T: false
    pub fn traverse_mut(
        &mut self,
        scope: &ScopePointer<T>,
        walker: &mut impl FnMut(&mut Self, &ScopePointer<T>, bool),
        is_top_level_union: bool,
    ) {
        let self_is_union = matches!(self, Self::Union(_, _));
        walker(self, scope, if self_is_union { false } else { is_top_level_union });

        let (_, scope) = self.build_uninferred_child_scope(scope);

        match self {
            Self::Operation { a, b, .. } => {
                a.traverse_mut(&scope, walker, false);
                b.traverse_mut(&scope, walker, false);
            }

            Self::Index { expr, index } => {
                expr.traverse_mut(&scope, walker, false);
                index.traverse_mut(&scope, walker, false);
            }

            Self::KeyOf(expr) => expr.traverse_mut(&scope, walker, false),

            Self::Union(a, b) => {
                a.traverse_mut(&scope, walker, is_top_level_union);
                b.traverse_mut(&scope, walker, is_top_level_union);
            }

            Self::Intersection(a, b) => {
                a.traverse_mut(&scope, walker, false);
                b.traverse_mut(&scope, walker, false);
            }

            Self::Type(_) => (),

            // Type variables are immutable!
            Self::Ref(ScopedTypeRef::Param(_)) => (),

            Self::Ref(ScopedTypeRef::ScopedExpr { expr, scope: portal }) => expr.traverse_mut(portal, walker, false),

            Self::Constructor { parameters, .. } => {
                parameters.values_mut().for_each(|p| p.traverse_mut(&scope, walker, false))
            }

            Self::NodeSignature(sig) => {
                sig.inputs.traverse_mut(&scope, walker, false);
                sig.outputs.traverse_mut(&scope, walker, false);
                sig.parameters
                    .values_mut()
                    .flat_map(|param| param.bound.iter_mut().chain(param.default.iter_mut()))
                    .for_each(|t| t.traverse_mut(&scope, walker, false));
            }

            Self::PortTypes(pt) => {
                pt.iter_mut().for_each(|t| t.traverse_mut(&scope, walker, false));
            }

            Self::Conditional(conditional) => {
                conditional.t_test.traverse_mut(&scope, walker, false);
                conditional.t_test_bound.traverse_mut(&scope, walker, false);
                conditional.t_then.traverse_mut(&scope, walker, false);
                conditional.t_else.traverse_mut(&scope, walker, false);
            }

            Self::Any => (),
            Self::Never => (),
        }
    }

    pub fn traverse(
        &self,
        scope: &ScopePointer<T>,
        walker: &mut impl FnMut(&Self, &ScopePointer<T>, bool),
        is_top_level_union: bool,
    ) {
        let self_is_union = matches!(self, Self::Union(_, _));
        walker(self, scope, if self_is_union { false } else { is_top_level_union });

        let (_, scope) = self.build_uninferred_child_scope(scope);

        match self {
            Self::Operation { a, b, .. } => {
                a.traverse(&scope, walker, false);
                b.traverse(&scope, walker, false);
            }

            Self::Index { expr, index } => {
                expr.traverse(&scope, walker, false);
                index.traverse(&scope, walker, false);
            }

            Self::KeyOf(expr) => expr.traverse(&scope, walker, false),

            Self::Union(a, b) => {
                a.traverse(&scope, walker, is_top_level_union);
                b.traverse(&scope, walker, is_top_level_union);
            }

            Self::Intersection(a, b) => {
                a.traverse(&scope, walker, false);
                b.traverse(&scope, walker, false);
            }

            Self::Type(_) => (),

            Self::Ref(ScopedTypeRef::Param(param)) => {
                let Some((inferred, inferred_scope)) = scope.lookup_inferred(&param.param_id) else {
                    return;
                };
                inferred.traverse(&inferred_scope, walker, is_top_level_union);
            }

            Self::Ref(ScopedTypeRef::ScopedExpr { expr, scope: portal }) => expr.traverse(portal, walker, false),

            Self::Constructor { parameters, .. } => parameters.values().for_each(|p| p.traverse(&scope, walker, false)),

            Self::NodeSignature(sig) => {
                sig.inputs.traverse(&scope, walker, false);
                sig.outputs.traverse(&scope, walker, false);
                sig.parameters
                    .values()
                    .flat_map(|param| param.bound.iter().chain(param.default.iter()))
                    .for_each(|t| t.traverse(&scope, walker, false));
            }

            Self::PortTypes(pt) => {
                pt.iter().for_each(|t| t.traverse(&scope, walker, false));
            }

            Self::Conditional(conditional) => {
                conditional.t_test.traverse(&scope, walker, false);
                conditional.t_test_bound.traverse(&scope, walker, false);
                conditional.t_then.traverse(&scope, walker, false);
                conditional.t_else.traverse(&scope, walker, false);
            }

            Self::Any => (),
            Self::Never => (),
        }
    }

    /// Used for candidate search.
    /// if infer_other is true, `self` will be used to infer parameters in `other`.
    pub(crate) fn traverse_parallel(
        &self,
        other: &Self,
        own_scope: &ScopePointer<T>,
        other_scope: &ScopePointer<T>,
        infer_other: bool,
        walker: &mut impl FnMut(
            &Self,            // own_type
            &Self,            // other_type
            &ScopePointer<T>, // current own scope
            &ScopePointer<T>, // current other scope (potentially inferred from `self`)
        ),
    ) {
        walker(self, other, own_scope, other_scope);

        let (own, own_scope) = self.build_uninferred_child_scope(own_scope);
        let (other, other_scope) = if infer_other {
            other.build_inferred_child_scope(own.as_ref(), other_scope, &own_scope)
        } else {
            other.build_uninferred_child_scope(other_scope)
        };

        match (own.as_ref(), other.as_ref()) {
            // Unions first so that during candidate collection type type type params get visited by all union variants before being looked up.
            (Self::Union(own_a, own_b), other) => {
                own_a.traverse_parallel(other, &own_scope, &other_scope, infer_other, walker);
                own_b.traverse_parallel(other, &own_scope, &other_scope, infer_other, walker);
            }

            (own, Self::Union(other_a, other_b)) => {
                own.traverse_parallel(other_a, &own_scope, &other_scope, infer_other, walker);
                own.traverse_parallel(other_b, &own_scope, &other_scope, infer_other, walker);
            }

            (Self::Operation { a, b, operator }, other) => {
                let (Some(a_normalized), Some(b_normalized)) =
                    (a.normalize_concrete(&own_scope), b.normalize_concrete(&own_scope))
                else {
                    return;
                };
                TypeExpr::from(T::operation(&a_normalized, operator, &b_normalized)).traverse_parallel(
                    other,
                    &own_scope,
                    &other_scope,
                    infer_other,
                    walker,
                );
            }
            (own, Self::Operation { a, b, operator }) => {
                let (Some(a_normalized), Some(b_normalized)) =
                    (a.normalize_concrete(&other_scope), b.normalize_concrete(&other_scope))
                else {
                    return;
                };
                own.traverse_parallel(
                    &T::operation(&a_normalized, operator, &b_normalized).into(),
                    &other_scope,
                    &own_scope,
                    infer_other,
                    walker,
                );
            }

            (Self::Ref(own_ref), other) => match own_ref {
                ScopedTypeRef::Param(param) => {
                    let Some((own_inferred, own_inferred_scope)) = own_scope.lookup_inferred(&param.param_id) else {
                        return;
                    };
                    own_inferred.traverse_parallel(other, &own_inferred_scope, &other_scope, infer_other, walker);
                }
                ScopedTypeRef::ScopedExpr { expr, scope } => {
                    expr.traverse_parallel(other, scope, &other_scope, infer_other, walker)
                }
            },
            (own, Self::Ref(other_ref)) => match other_ref {
                ScopedTypeRef::Param(param) => {
                    let Some((other_inferred, other_inferred_scope)) = other_scope.lookup_inferred(&param.param_id)
                    else {
                        return;
                    };
                    own.traverse_parallel(&other_inferred, &own_scope, &other_inferred_scope, infer_other, walker);
                }
                ScopedTypeRef::ScopedExpr { expr, scope } => {
                    own.traverse_parallel(expr, &own_scope, scope, infer_other, walker)
                }
            },

            (Self::KeyOf(own_expr), other) => {
                let Some((keyof, keyof_scope)) = own_expr.keyof(&own_scope) else {
                    return;
                };
                keyof.traverse_parallel(other, &keyof_scope, &other_scope, infer_other, walker);
            }

            (own, Self::KeyOf(other_expr)) => {
                let Some((keyof, keyof_scope)) = other_expr.keyof(&other_scope) else {
                    return;
                };
                own.traverse_parallel(&keyof, &own_scope, &keyof_scope, infer_other, walker);
            }

            (Self::Intersection(own_a, own_b), other) => {
                if let Some((own_intersection, own_intersection_scope)) =
                    Self::intersection(own_a, own_b, &own_scope, &own_scope)
                {
                    own_intersection.traverse_parallel(
                        other,
                        &own_intersection_scope,
                        &other_scope,
                        infer_other,
                        walker,
                    );
                }
            }

            (own, Self::Intersection(other_a, other_b)) => {
                if let Some((other_intersection, other_intersection_scope)) =
                    Self::intersection(other_a, other_b, &other_scope, &other_scope)
                {
                    own.traverse_parallel(
                        &other_intersection,
                        &own_scope,
                        &other_intersection_scope,
                        infer_other,
                        walker,
                    );
                }
            }

            (Self::Type(_), Self::Type(_)) => (),

            (
                Self::Constructor { parameters: own_params, inner: own_inner },
                Self::Constructor { parameters: other_params, inner: other_inner },
            ) => {
                if !own_inner.supertype_of(other_inner) {
                    return;
                }
                // Traverse over all common params
                for (key, own_param) in own_params {
                    if let Some(other_param) = other_params.get(key) {
                        own_param.traverse_parallel(other_param, &own_scope, &other_scope, infer_other, walker);
                    }
                }
            }

            (Self::NodeSignature(own_signature), Self::NodeSignature(other_signature)) => {
                own_signature.inputs.traverse_parallel(
                    &other_signature.inputs,
                    &own_scope,
                    &other_scope,
                    infer_other,
                    walker,
                );
                own_signature.outputs.traverse_parallel(
                    &other_signature.outputs,
                    &own_scope,
                    &other_scope,
                    infer_other,
                    walker,
                );
                // What to do with type parameters?
            }

            (Self::PortTypes(own_ports), Self::PortTypes(other_ports)) => {
                let max_arg_count = own_ports.ports.len().max(other_ports.ports.len()) + 1;
                // Each port index (ports+varg) that is present in both is visited at least once.
                let mut i = 0;
                while let (Some(own_port), Some(other_port)) =
                    (own_ports.get_port_type(i), other_ports.get_port_type(i))
                {
                    own_port.traverse_parallel(other_port, &own_scope, &other_scope, infer_other, walker);
                    i += 1;
                    if i >= max_arg_count {
                        break; // In case both have variadic ports.
                    }
                }
            }

            (Self::Index { expr, index }, other) => {
                let Some((own_idx, own_idx_scope)) = expr.index(index, &own_scope, &own_scope) else {
                    return;
                };
                own_idx.traverse_parallel(other, &own_idx_scope, &other_scope, infer_other, walker)
            }
            (own, Self::Index { expr, index }) => {
                let Some((other_idx, other_idx_scope)) = expr.index(index, &other_scope, &other_scope) else {
                    return;
                };
                own.traverse_parallel(&other_idx, &own_scope, &other_idx_scope, infer_other, walker)
            }

            (Self::Conditional(own_conditional), other) => {
                let Some(distributed) = own_conditional.distribute(&own_scope) else {
                    return;
                };
                distributed.traverse_parallel(other, &own_scope, &other_scope, infer_other, walker)
            }
            (own, Self::Conditional(other_conditional)) => {
                let Some(distributed) = other_conditional.distribute(&other_scope) else {
                    return;
                };
                own.traverse_parallel(&distributed, &own_scope, &other_scope, infer_other, walker)
            }

            (Self::Any | Self::Never, _) => (),
            (_, Self::Any | Self::Never) => (),
            (_, Self::PortTypes(_)) => (),
            (Self::PortTypes(_), _) => (),
            (Self::Constructor { .. }, Self::Type(_)) => (),
            (Self::Type(_), Self::Constructor { .. }) => (),
            (Self::Type(_), Self::NodeSignature(_)) => (),
            (Self::Constructor { .. }, Self::NodeSignature(_)) => (),
            (Self::NodeSignature(_), Self::Type(_)) => (),
            (Self::NodeSignature(_), Self::Constructor { .. }) => (),
        }
    }
}
