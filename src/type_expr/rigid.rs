//! Rigid type parameters (see [TypeParameter::rigid](crate::scope::type_parameter::TypeParameter::rigid)) and the
//! operations over them.
use crate::{
    scope::{LocalParamID, ScopePointer},
    r#type::Type,
    type_expr::{
        AsScopedRef, ConcreteTypeExpr, NoRef, ParamRef, ScopedRefView, ScopedTypeExpr, ScopedTypeRef, TypeExpr,
        TypeRef, type_ref::private,
    },
};
use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
};

/// A reference to a rigid type parameter. The only reference [Type::operation] gets to see.
///
/// Ordered by the parameter's id first and then by its scope, so that the operands of an operation sort the same way
/// in every run as long as the rigid parameters of different scopes have different ids.
#[derive(Debug, Clone)]
pub struct RigidRef<T: Type> {
    /// `Ref(param_id)`, to be read in `scope`.
    expr: Box<ScopedTypeExpr<T>>,
    param_id: LocalParamID,
    scope: ScopePointer<T>,
}

/// An expression over rigid parameters, see [RigidRef].
pub type RigidTypeExpr<T> = TypeExpr<T, RigidRef<T>>;

/// The references [Type::operation] can get: none, or rigid parameters. Sealed.
///
/// [Type::operation] can't create one, it can only pass on those it got.
pub trait OperandRef<T: Type>: AsScopedRef<T> + Eq + Ord + Hash {}

impl<T: Type> OperandRef<T> for NoRef {}
impl<T: Type> OperandRef<T> for RigidRef<T> {}

impl<T: Type> RigidRef<T> {
    fn new(param_id: LocalParamID, scope: ScopePointer<T>) -> Self {
        Self { expr: Box::new(TypeExpr::Ref(ScopedTypeRef::Param(ParamRef::new(param_id, false)))), param_id, scope }
    }

    pub fn param_id(&self) -> LocalParamID {
        self.param_id
    }

    /// The scope that defines the parameter.
    pub fn scope(&self) -> &ScopePointer<T> {
        &self.scope
    }

    fn key(&self) -> (LocalParamID, usize) {
        (self.param_id, self.scope.address())
    }
}

impl<T: Type> PartialEq for RigidRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl<T: Type> Eq for RigidRef<T> {}

impl<T: Type> Hash for RigidRef<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

impl<T: Type> PartialOrd for RigidRef<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Type> Ord for RigidRef<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl<T: Type> private::Sealed for RigidRef<T> {}

impl<T: Type> TypeRef for RigidRef<T> {
    fn as_param_ref(&self) -> Option<&ParamRef> {
        None
    }
}

impl<T: Type> AsScopedRef<T> for RigidRef<T> {
    fn view(&self) -> ScopedRefView<'_, T> {
        ScopedRefView::ScopedExpr { expr: &self.expr, scope: &self.scope }
    }

    fn into_scoped_ref(self) -> ScopedTypeRef<T> {
        ScopedTypeRef::ScopedExpr { expr: self.expr, scope: self.scope }
    }
}

impl<T: Type, R: AsScopedRef<T>> TypeExpr<T, R> {
    /// `self` as an expression over rigid parameters, with its operations computed by [Type::operation].
    ///
    /// # Returns
    /// `None` if `self` references an uninferred parameter, or if it is neither an operation, a rigid parameter nor
    /// concrete.
    pub fn rigid_form(&self, scope: &ScopePointer<T>) -> Option<RigidTypeExpr<T>> {
        match self {
            Self::Operation { a, operator, b } => {
                Some(T::operation(&a.rigid_form(scope)?, operator, &b.rigid_form(scope)?))
            }
            Self::Ref(r) => match r.view() {
                ScopedRefView::Param(param) => {
                    let (var, param_scope) = scope.lookup(&param.param_id)?;
                    if let Some((inferred, inferred_scope)) = var.inferred() {
                        return inferred.rigid_form(&inferred_scope);
                    }
                    var.is_rigid()
                        .then(|| TypeExpr::Ref(RigidRef::new(param.param_id, ScopePointer::clone(param_scope))))
                }
                ScopedRefView::ScopedExpr { expr, scope } => expr.rigid_form(scope),
            },
            other => Some(other.normalize_concrete(scope)?.map_refs(|never: NoRef| match never {})),
        }
    }

    /// The [rigid_form](Self::rigid_form) of `self` if it references a rigid parameter, so that it is a supertype only
    /// of the same form. Only a rigid parameter itself or an operation can have one.
    pub(crate) fn rigid_parent_form(&self, scope: &ScopePointer<T>) -> Option<RigidTypeExpr<T>> {
        let candidate = match self {
            Self::Operation { .. } => true,
            Self::Ref(r) => match r.view() {
                ScopedRefView::Param(param) => scope.lookup(&param.param_id).is_some_and(|(var, _)| var.is_rigid()),
                ScopedRefView::ScopedExpr { .. } => false,
            },
            _ => false,
        };
        if !candidate {
            return None;
        }
        self.rigid_form(scope).filter(|form| form.clone().try_into_concrete().is_err())
    }

    /// Like [normalize_concrete](Self::normalize_concrete), but every rigid parameter in an operation is replaced by
    /// its bound: what `self` is as a child.
    ///
    /// This is an upper bound of `self` as long as [Type::operation] is monotone.
    pub(crate) fn normalize_concrete_widened(&self, scope: &ScopePointer<T>) -> Option<ConcreteTypeExpr<T>> {
        match self {
            Self::Operation { a, operator, b } => Some(T::operation(
                &a.normalize_concrete_widened(scope)?,
                operator,
                &b.normalize_concrete_widened(scope)?,
            )),
            Self::Ref(r) => match r.view() {
                ScopedRefView::Param(param) => {
                    let (var, param_scope) = scope.lookup(&param.param_id)?;
                    if let Some((inferred, inferred_scope)) = var.inferred() {
                        return inferred.normalize_concrete_widened(&inferred_scope);
                    }
                    if !var.is_rigid() {
                        return None;
                    }
                    let (bound, bound_scope) = var.get_boundary(param_scope);
                    bound.normalize_concrete_widened(&bound_scope)
                }
                ScopedRefView::ScopedExpr { expr, scope } => expr.normalize_concrete_widened(scope),
            },
            other => other.normalize_concrete(scope),
        }
    }
}
