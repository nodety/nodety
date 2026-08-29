use crate::{
    scope::{ScopePointer, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{
        AsScopedRef, ConcreteTypeExpr, NoRef, ParamRef, ParameterizedTypeExpr, ScopedTypeExpr, ScopedTypeRef, TypeExpr,
        TypeRef,
        conditional::Conditional,
        node_signature::{NodeSignature, port_types::PortTypes, type_parameters::TypeParameters},
    },
};

/// The expression contains at least one scope portal and can therefore not be represented without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HasScopePortals;

/// The expression references at least one type parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HasTypeParameters;

macro_rules! ref_mappers {
    ($t:ident) => {
        impl<T: Type, R: AsScopedRef<T>> $t<T, R> {
            /// Widens `self` into the scope aware representation used internally.
            pub fn into_scoped(self) -> $t<T, ScopedTypeRef<T>> {
                self.map_refs(&mut |r: R| r.into_scoped_ref())
            }
        }

        impl<T: Type, R: TypeRef> $t<T, R> {
            /// Infallible version of [Self::try_map_refs].
            pub fn map_refs<RO: TypeRef>(self, mapper: &mut impl FnMut(R) -> RO) -> $t<T, RO> {
                self.try_map_refs::<RO, std::convert::Infallible>(&mut |r| Ok(mapper(r))).unwrap_or_else(|e| match e {})
            }

            /// Attempts to convert `self` into an expression that only references type parameters.
            ///
            /// Does not modify the expression in any way.
            ///
            /// # Errors
            /// if the expression contains one or more scope portals.
            pub fn try_into_unscoped(self) -> Result<$t<T, ParamRef>, HasScopePortals> {
                self.try_map_refs(&mut |r: R| r.as_param_ref().copied().ok_or(HasScopePortals))
            }

            /// Attempts to convert `self` into an expression that references nothing at all.
            ///
            /// Does not modify the expression in any way.
            ///
            /// # Errors
            /// if the expression contains any reference.
            pub fn try_into_concrete(self) -> Result<$t<T, NoRef>, HasTypeParameters> {
                self.try_map_refs(&mut |_: R| Err(HasTypeParameters))
            }
        }
    };
}

ref_mappers!(TypeExpr);
ref_mappers!(NodeSignature);
ref_mappers!(PortTypes);
ref_mappers!(TypeParameter);
ref_mappers!(TypeParameters);

// Widening conversions. All of these are lossless.
macro_rules! widening {
    ($t:ident) => {
        impl<T: Type> From<$t<T, NoRef>> for $t<T, ParamRef> {
            fn from(value: $t<T, NoRef>) -> Self {
                value.map_refs(&mut |never| match never {})
            }
        }

        impl<T: Type> From<$t<T, NoRef>> for $t<T, ScopedTypeRef<T>> {
            fn from(value: $t<T, NoRef>) -> Self {
                value.map_refs(&mut |never| match never {})
            }
        }

        impl<T: Type> From<$t<T, ParamRef>> for $t<T, ScopedTypeRef<T>> {
            fn from(value: $t<T, ParamRef>) -> Self {
                value.map_refs(&mut ScopedTypeRef::Param)
            }
        }
    };
}

widening!(TypeExpr);
widening!(NodeSignature);
widening!(PortTypes);
widening!(TypeParameter);
widening!(TypeParameters);

impl<T: Type, R: TypeRef> TypeExpr<T, R> {
    /// Rebuilds `self` with every [TypeExpr::Ref] replaced by `mapper`'s result.
    ///
    /// **Note:** for [ScopedTypeRef::ScopedExpr] the mapper receives the *whole* reference,
    /// nested expression included. Mapping into a different reference kind therefore has to
    /// recurse into that expression itself.
    pub fn try_map_refs<RO: TypeRef, E>(
        self,
        mapper: &mut impl FnMut(R) -> Result<RO, E>,
    ) -> Result<TypeExpr<T, RO>, E> {
        Ok(match self {
            Self::Type(t) => TypeExpr::Type(t),
            Self::Constructor { inner, parameters } => TypeExpr::Constructor {
                inner,
                parameters: parameters
                    .into_iter()
                    .map(|(k, v)| Ok((k, v.try_map_refs(mapper)?)))
                    .collect::<Result<_, E>>()?,
            },
            Self::Operation { a, operator, b } => TypeExpr::Operation {
                a: Box::new(a.try_map_refs(mapper)?),
                operator,
                b: Box::new(b.try_map_refs(mapper)?),
            },
            Self::NodeSignature(sig) => TypeExpr::NodeSignature(Box::new(sig.try_map_refs(mapper)?)),
            Self::PortTypes(pt) => TypeExpr::PortTypes(Box::new(pt.try_map_refs(mapper)?)),
            Self::Union(a, b) => TypeExpr::Union(Box::new(a.try_map_refs(mapper)?), Box::new(b.try_map_refs(mapper)?)),
            Self::KeyOf(expr) => TypeExpr::KeyOf(Box::new(expr.try_map_refs(mapper)?)),
            Self::Index { expr, index } => TypeExpr::Index {
                expr: Box::new(expr.try_map_refs(mapper)?),
                index: Box::new(index.try_map_refs(mapper)?),
            },
            Self::Intersection(a, b) => {
                TypeExpr::Intersection(Box::new(a.try_map_refs(mapper)?), Box::new(b.try_map_refs(mapper)?))
            }
            Self::Conditional(conditional) => TypeExpr::Conditional(Box::new(Conditional {
                t_test: conditional.t_test.try_map_refs(mapper)?,
                t_test_bound: conditional.t_test_bound.try_map_refs(mapper)?,
                t_then: conditional.t_then.try_map_refs(mapper)?,
                t_else: conditional.t_else.try_map_refs(mapper)?,
                infer: conditional.infer,
            })),
            Self::Any => TypeExpr::Any,
            Self::Never => TypeExpr::Never,
            Self::Ref(r) => TypeExpr::Ref(mapper(r)?),
        })
    }
}

impl<T: Type> ScopedTypeExpr<T> {
    /// Tries to remove all scope portals from the expression, leaving behind an expression that
    /// only references type parameters.
    ///
    /// # Errors
    /// When there is at least one scope portal whose expression contains a type parameter.
    pub fn try_remove_scope_portals(mut self) -> Result<ParameterizedTypeExpr<T>, HasTypeParameters> {
        let mut failed = false;
        self.traverse_mut(
            &ScopePointer::new_root(),
            &mut |expr, _scope, _is_tl_union| {
                if let TypeExpr::Ref(ScopedTypeRef::ScopedExpr { expr: inner_expr, .. }) = expr {
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

    /// Removes all scope portals from the expression, leaving behind an expression that only
    /// references type parameters. Intended for user display.
    ///
    /// # Soundness
    /// Be aware that this is an unsound operation and the semantic meaning of the type might change.
    pub fn force_remove_scope_portals(mut self) -> ParameterizedTypeExpr<T> {
        self.traverse_mut(
            &ScopePointer::new_root(),
            &mut |expr, _scope, _is_tl_union| {
                if let TypeExpr::Ref(ScopedTypeRef::ScopedExpr { expr: inner_expr, .. }) = expr {
                    *expr = std::mem::take(inner_expr);
                }
            },
            true,
        );
        self.try_into_unscoped().expect("Expected no portals to remain after removing all")
    }
}

impl<T: Type, R: AsScopedRef<T>> TypeExpr<T, R> {
    /// Normalizes `self` and, if nothing is left referencing the outside, returns the result as a
    /// [ConcreteTypeExpr].
    ///
    /// This is what feeds the [Type](crate::Type) trait: implementors only ever get to see types
    /// they can fully understand without a scope.
    ///
    /// # Returns
    /// `None` if the normalized expression still references an (uninferred) type parameter.
    pub fn normalize_concrete(&self, scope: &ScopePointer<T>) -> Option<ConcreteTypeExpr<T>> {
        self.normalize(scope).try_remove_scope_portals().ok()?.try_into_concrete().ok()
    }
}

impl<T: Type, R: TypeRef> PortTypes<T, R> {
    pub fn try_map_refs<RO: TypeRef, E>(
        self,
        mapper: &mut impl FnMut(R) -> Result<RO, E>,
    ) -> Result<PortTypes<T, RO>, E> {
        Ok(PortTypes {
            ports: self.ports.into_iter().map(|p| p.try_map_refs(mapper)).collect::<Result<_, E>>()?,
            varg: self.varg.map(|v| v.try_map_refs(mapper)).transpose()?,
        })
    }
}

impl<T: Type, R: TypeRef> TypeParameter<T, R> {
    pub fn try_map_refs<RO: TypeRef, E>(
        self,
        mapper: &mut impl FnMut(R) -> Result<RO, E>,
    ) -> Result<TypeParameter<T, RO>, E> {
        Ok(TypeParameter {
            bound: self.bound.map(|bound| bound.try_map_refs(mapper)).transpose()?,
            default: self.default.map(|default| default.try_map_refs(mapper)).transpose()?,
        })
    }
}

impl<T: Type, R: TypeRef> TypeParameters<T, R> {
    pub fn try_map_refs<RO: TypeRef, E>(
        self,
        mapper: &mut impl FnMut(R) -> Result<RO, E>,
    ) -> Result<TypeParameters<T, RO>, E> {
        self.0
            .into_iter()
            .map(|(k, param)| Ok((k, param.try_map_refs(mapper)?)))
            .collect::<Result<_, E>>()
            .map(TypeParameters)
    }
}

impl<T: Type, R: TypeRef> NodeSignature<T, R> {
    pub fn try_map_refs<RO: TypeRef, E>(
        self,
        mapper: &mut impl FnMut(R) -> Result<RO, E>,
    ) -> Result<NodeSignature<T, RO>, E> {
        Ok(NodeSignature {
            parameters: self.parameters.try_map_refs(mapper)?,
            inputs: self.inputs.try_map_refs(mapper)?,
            outputs: self.outputs.try_map_refs(mapper)?,
            default_input_types: self
                .default_input_types
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_map_refs(mapper)?)))
                .collect::<Result<_, E>>()?,
            tags: self.tags,
            required_tags: self.required_tags,
        })
    }
}
