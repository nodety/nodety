use crate::{
    scope::{GlobalParameterId, ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{
        ErasedScopePortal, ScopePortal, TypeExpr, TypeExprScope, Unscoped, UnscopedTypeExpr,
        conditional::Conditional,
        node_signature::{NodeSignature, port_types::PortTypes, type_parameters::TypeParameters},
    },
};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HasScopePortals;

// Type expr conversions

impl<T: Type> From<TypeExpr<T, ScopePortal<T>>> for TypeExpr<T, ErasedScopePortal> {
    fn from(value: TypeExpr<T, ScopePortal<T>>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<TypeExpr<T, Unscoped>> for TypeExpr<T, ErasedScopePortal> {
    fn from(value: TypeExpr<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<TypeExpr<T, Unscoped>> for TypeExpr<T, ScopePortal<T>> {
    fn from(value: TypeExpr<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |never| match never {})
    }
}

impl<T: Type> TypeExpr<T, Unscoped> {
    /// Converts an unscoped type expression into a scoped one.
    pub fn into_scoped(self) -> TypeExpr<T, ScopePortal<T>> {
        self.into()
    }
}

// Type parameter conversions

impl<T: Type> From<TypeParameter<T, ScopePortal<T>>> for TypeParameter<T, ErasedScopePortal> {
    fn from(value: TypeParameter<T, ScopePortal<T>>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<TypeParameter<T, Unscoped>> for TypeParameter<T, ErasedScopePortal> {
    fn from(value: TypeParameter<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<TypeParameter<T, Unscoped>> for TypeParameter<T, ScopePortal<T>> {
    fn from(value: TypeParameter<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |never| match never {})
    }
}

impl<T: Type> TypeParameter<T, Unscoped> {
    /// Converts an unscoped type parameter into a scoped one.
    pub fn into_scoped(self) -> TypeParameter<T, ScopePortal<T>> {
        self.into()
    }
}

// Type parameters

impl<T: Type> From<TypeParameters<T, ScopePortal<T>>> for TypeParameters<T, ErasedScopePortal> {
    fn from(value: TypeParameters<T, ScopePortal<T>>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<TypeParameters<T, Unscoped>> for TypeParameters<T, ErasedScopePortal> {
    fn from(value: TypeParameters<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<TypeParameters<T, Unscoped>> for TypeParameters<T, ScopePortal<T>> {
    fn from(value: TypeParameters<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |never| match never {})
    }
}

impl<T: Type> TypeParameters<T, Unscoped> {
    /// Converts unscoped type parameters into scoped ones.
    pub fn into_scoped(self) -> TypeParameters<T, ScopePortal<T>> {
        self.into()
    }
}

// Node signature

impl<T: Type> From<NodeSignature<T, ScopePortal<T>>> for NodeSignature<T, ErasedScopePortal> {
    fn from(value: NodeSignature<T, ScopePortal<T>>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<NodeSignature<T, Unscoped>> for NodeSignature<T, ErasedScopePortal> {
    fn from(value: NodeSignature<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |_| ErasedScopePortal)
    }
}

impl<T: Type> From<NodeSignature<T, Unscoped>> for NodeSignature<T, ScopePortal<T>> {
    fn from(value: NodeSignature<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |never| match never {})
    }
}

impl<T: Type> NodeSignature<T, Unscoped> {
    /// Converts an unscoped node signature into a scoped one.
    pub fn into_scoped(self) -> NodeSignature<T, ScopePortal<T>> {
        self.into()
    }
}

// Port types

impl<T: Type> From<PortTypes<T, Unscoped>> for PortTypes<T, ScopePortal<T>> {
    fn from(value: PortTypes<T, Unscoped>) -> Self {
        value.map_scope_portals(&mut |never| match never {})
    }
}

impl<T: Type> PortTypes<T, Unscoped> {
    /// Converts unscoped port types into scoped ones.
    pub fn into_scoped(self) -> PortTypes<T, ScopePortal<T>> {
        self.into()
    }
}

impl<T: Type, S: TypeExprScope> TypeExpr<T, S> {
    pub fn try_map_scope_portals<SO: TypeExprScope, E>(
        self,
        mapper: &mut impl FnMut(S) -> Result<SO, E>,
    ) -> Result<TypeExpr<T, SO>, E> {
        Ok(match self {
            Self::Type(t) => TypeExpr::Type(t),
            Self::Constructor { inner, parameters } => TypeExpr::Constructor {
                inner,
                parameters: parameters
                    .into_iter()
                    .map(|(k, v)| Ok((k, v.try_map_scope_portals(mapper)?)))
                    .collect::<Result<_, E>>()?,
            },
            Self::Operation { a, operator, b } => TypeExpr::Operation {
                a: Box::new(a.try_map_scope_portals(mapper)?),
                operator,
                b: Box::new(b.try_map_scope_portals(mapper)?),
            },
            Self::TypeParameter(id, infer) => TypeExpr::TypeParameter(id, infer),
            Self::NodeSignature(sig) => TypeExpr::NodeSignature(Box::new(sig.try_map_scope_portals(mapper)?)),
            Self::PortTypes(pt) => TypeExpr::PortTypes(Box::new(pt.try_map_scope_portals(mapper)?)),
            Self::Union(a, b) => {
                TypeExpr::Union(Box::new(a.try_map_scope_portals(mapper)?), Box::new(b.try_map_scope_portals(mapper)?))
            }
            Self::KeyOf(expr) => TypeExpr::KeyOf(Box::new(expr.try_map_scope_portals(mapper)?)),
            Self::Index { expr, index } => TypeExpr::Index {
                expr: Box::new(expr.try_map_scope_portals(mapper)?),
                index: Box::new(index.try_map_scope_portals(mapper)?),
            },
            Self::Intersection(a, b) => TypeExpr::Intersection(
                Box::new(a.try_map_scope_portals(mapper)?),
                Box::new(b.try_map_scope_portals(mapper)?),
            ),
            Self::Conditional(conditional) => TypeExpr::Conditional(Box::new(Conditional {
                t_test: conditional.t_test.try_map_scope_portals(mapper)?,
                t_test_bound: conditional.t_test_bound.try_map_scope_portals(mapper)?,
                t_then: conditional.t_then.try_map_scope_portals(mapper)?,
                t_else: conditional.t_else.try_map_scope_portals(mapper)?,
                infer: conditional.infer,
            })),
            Self::Any => TypeExpr::Any,
            Self::Never => TypeExpr::Never,
            Self::ScopePortal { expr, scope } => {
                TypeExpr::ScopePortal { expr: Box::new(expr.try_map_scope_portals(mapper)?), scope: mapper(scope)? }
            }
        })
    }

    /// Infallible version of [Self::try_map_scope_portals].
    pub fn map_scope_portals<SO: TypeExprScope>(self, mapper: &mut impl FnMut(S) -> SO) -> TypeExpr<T, SO> {
        self.try_map_scope_portals::<SO, std::convert::Infallible>(&mut |s| Ok(mapper(s)))
            .unwrap_or_else(|e| match e {})
    }

    /// Attempts to convert an expression into unscoped.
    /// Does not modify the expression in any way.
    /// # Errors
    /// if the expression contains one or more [ScopePortal][TypeExpr::ScopePortal], variant.
    pub fn try_into_unscoped(self) -> Result<TypeExpr<T, Unscoped>, HasScopePortals> {
        self.try_map_scope_portals(&mut |_| Err(HasScopePortals))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HasTypeParameters;

impl<T: Type> TypeExpr<T, ScopePortal<T>> {
    /// Tries to remove all scope portals from the expression, leaving behind an unscoped expression.
    /// # Errors
    /// When there is at least one [ScopePortal][TypeExpr::ScopePortal] whose expression contains a type parameter.
    pub fn try_remove_scope_portals(mut self) -> Result<UnscopedTypeExpr<T>, HasTypeParameters> {
        let mut failed = false;
        self.traverse_mut(
            &ScopePointer::new_root(),
            &mut |expr, _scope, _is_tl_union| {
                if let TypeExpr::ScopePortal { scope: _, expr: inner_expr } = expr {
                    if inner_expr.contains_type_param() {
                        failed = true;
                        // Quit traversal
                        *expr = TypeExpr::Any;
                    } else {
                        *expr = std::mem::take(inner_expr);
                    }
                }
            },
            true,
        );
        if failed {
            return Err(HasTypeParameters);
        }
        Ok(self.try_into_unscoped().expect("Expected no portals to remain after removing all"))
    }

    /// Removes all scope portals from the expression, leaving behind an unscoped expression.
    /// Intended for user display.
    ///
    /// # Soundness
    /// Be aware that this is an unsound operation and the semantic meaning of the type might change.
    pub fn force_remove_scope_portals(mut self) -> UnscopedTypeExpr<T> {
        self.traverse_mut(
            &ScopePointer::new_root(),
            &mut |expr, _scope, _is_tl_union| {
                if let TypeExpr::ScopePortal { scope: _, expr: inner_expr } = expr {
                    *expr = std::mem::take(inner_expr);
                }
            },
            true,
        );
        self.try_into_unscoped().expect("Expected no portals to remain after removing all")
    }

    fn replace_vars_by_normalized_bounds_inner(
        mut self,
        scope: &ScopePointer<T>,
        seen: &mut HashSet<GlobalParameterId<T>>,
    ) -> Self {
        self.traverse_mut(
            scope,
            &mut |expr, scope: &ScopePointer<T>, _is_tl_union| {
                if let TypeExpr::TypeParameter(param, _infer) = expr {
                    let Some((var, param_scope)) = scope.lookup(param) else {
                        *expr = TypeExpr::Any;
                        return;
                    };

                    if !seen.insert(GlobalParameterId { scope: param_scope.clone(), local_id: *param }) {
                        *expr = TypeExpr::Any;
                        return;
                    }

                    let (bound, bound_scope) = var.get_boundary(param_scope);
                    *expr = bound.into_owned().replace_vars_by_normalized_bounds_inner(&bound_scope, seen)
                }
            },
            true,
        );
        self
    }

    /// Replaces all type parameters in `self` by their bounds.
    /// The bound of a param is its inferred type or its bound or Any if neither is set.
    ///
    /// This is unsound but useful for displaying to a user that might not know about type variables.
    pub fn replace_vars_by_normalized_bounds(self, scope: &ScopePointer<T>) -> UnscopedTypeExpr<T> {
        let mut seen: HashSet<GlobalParameterId<T>> = HashSet::new();
        self.replace_vars_by_normalized_bounds_inner(scope, &mut seen)
            .try_into_unscoped()
            .expect("Expected there to be no type params left after removing all")
    }
}

impl<T: Type, S: TypeExprScope> PortTypes<T, S> {
    pub(crate) fn try_map_scope_portals<SO: TypeExprScope, E>(
        self,
        mapper: &mut impl FnMut(S) -> Result<SO, E>,
    ) -> Result<PortTypes<T, SO>, E> {
        Ok(PortTypes {
            ports: self.ports.into_iter().map(|p| p.try_map_scope_portals(mapper)).collect::<Result<_, E>>()?,
            varg: self.varg.map(|v| v.try_map_scope_portals(mapper)).transpose()?,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn map_scope_portals<SO: TypeExprScope>(self, mapper: &mut impl FnMut(S) -> SO) -> PortTypes<T, SO> {
        self.try_map_scope_portals::<SO, std::convert::Infallible>(&mut |s| Ok(mapper(s)))
            .unwrap_or_else(|e| match e {})
    }
}

impl<T: Type, S: TypeExprScope> TypeParameter<T, S> {
    fn try_map_scope_portals<SO: TypeExprScope, E>(
        self,
        mapper: &mut impl FnMut(S) -> Result<SO, E>,
    ) -> Result<TypeParameter<T, SO>, E> {
        Ok(TypeParameter {
            bound: self.bound.map(|bound| bound.try_map_scope_portals(mapper)).transpose()?,
            default: self.default.map(|default| default.try_map_scope_portals(mapper)).transpose()?,
        })
    }

    fn map_scope_portals<SO: TypeExprScope>(self, mapper: &mut impl FnMut(S) -> SO) -> TypeParameter<T, SO> {
        self.try_map_scope_portals::<SO, std::convert::Infallible>(&mut |s| Ok(mapper(s)))
            .unwrap_or_else(|e| match e {})
    }
}

impl<T: Type, S: TypeExprScope> TypeParameters<T, S> {
    pub(crate) fn try_map_scope_portals<SO: TypeExprScope, E>(
        self,
        mapper: &mut impl FnMut(S) -> Result<SO, E>,
    ) -> Result<TypeParameters<T, SO>, E> {
        self.0
            .into_iter()
            .map(|(k, param)| Ok((k, param.try_map_scope_portals(mapper)?)))
            .collect::<Result<_, E>>()
            .map(TypeParameters)
    }

    pub(crate) fn map_scope_portals<SO: TypeExprScope>(
        self,
        mapper: &mut impl FnMut(S) -> SO,
    ) -> TypeParameters<T, SO> {
        self.try_map_scope_portals::<SO, std::convert::Infallible>(&mut |s| Ok(mapper(s)))
            .unwrap_or_else(|e| match e {})
    }
}

impl<T: Type, S: TypeExprScope> NodeSignature<T, S> {
    pub(crate) fn try_map_scope_portals<SO: TypeExprScope, E>(
        self,
        mapper: &mut impl FnMut(S) -> Result<SO, E>,
    ) -> Result<NodeSignature<T, SO>, E> {
        Ok(NodeSignature {
            parameters: self.parameters.try_map_scope_portals(mapper)?,
            inputs: self.inputs.try_map_scope_portals(mapper)?,
            outputs: self.outputs.try_map_scope_portals(mapper)?,
            default_input_types: self
                .default_input_types
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_map_scope_portals(mapper)?)))
                .collect::<Result<_, E>>()?,
            tags: self.tags,
            required_tags: self.required_tags,
        })
    }

    pub(crate) fn map_scope_portals<SO: TypeExprScope>(self, mapper: &mut impl FnMut(S) -> SO) -> NodeSignature<T, SO> {
        self.try_map_scope_portals::<SO, std::convert::Infallible>(&mut |s| Ok(mapper(s)))
            .unwrap_or_else(|e| match e {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        notation::parse::{expr, expr_u},
        scope::Scope,
    };

    /// Regression test:
    /// a self-referential bound (`C extends C & Boolean`) caused traversal
    /// to recurse into substituted bounds.
    #[test]
    fn replace_vars_by_bounds_handles_self_referential_bound() {
        let recursive_bound = expr("C & Boolean");

        let mut root = Scope::new_root();
        root.define("C".into(), TypeParameter { bound: Some(recursive_bound), default: None });
        let scope = ScopePointer::new(root);

        let expr1 = TypeExpr::TypeParameter("C".into(), true);

        assert_eq!(expr1.replace_vars_by_normalized_bounds(&scope), expr_u("Any & Boolean"));
    }
}
