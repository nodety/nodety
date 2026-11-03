//! Rigid type parameters (see [TypeParameter::rigid](crate::scope::type_parameter::TypeParameter::rigid)) and the
//! linear normal form that operations over them are compared in.
use crate::{
    scope::{GlobalParameterId, ScopePointer},
    r#type::Type,
    type_expr::{AsScopedRef, ConcreteTypeExpr, ScopedRefView, TypeExpr},
};
use std::collections::HashMap;

/// The operators of [Type::operation] that add and subtract, and the neutral element of adding.
/// See [Type::linear_arithmetic].
#[derive(Debug, Clone, PartialEq)]
pub struct LinearArithmetic<T: Type> {
    pub add: T::Operator,
    pub subtract: T::Operator,
    pub zero: ConcreteTypeExpr<T>,
}

/// `Σ coefficient · parameter + constant`: an expression over rigid parameters in normal form.
///
/// `N + 1`, `1 + N` and `(N + 3) - 2` all have the terms `{N: 1}` and the constant `1`.
#[derive(Debug, Clone)]
pub struct LinearForm<T: Type> {
    /// The rigid parameters with their coefficients, none of them 0.
    pub terms: HashMap<GlobalParameterId<T>, i64>,
    /// `None` is the neutral element of adding, so that a single rigid parameter needs no [LinearArithmetic].
    pub constant: Option<ConcreteTypeExpr<T>>,
}

impl<T: Type> LinearForm<T> {
    /// Whether every value of `child` is a value of `self`: the same terms, and the constant of `self` a supertype
    /// of the constant of `child`.
    pub fn supertype_of(&self, child: &Self) -> bool {
        if self.terms != child.terms {
            return false;
        }
        let zero = || T::linear_arithmetic().map(|arithmetic| arithmetic.zero);
        match (self.constant.clone().or_else(zero), child.constant.clone().or_else(zero)) {
            (Some(parent), Some(child)) => parent.supertype_of_naive(&child).is_supertype(),
            (None, None) => true,
            _ => false,
        }
    }

    fn constant(constant: ConcreteTypeExpr<T>) -> Self {
        Self { terms: HashMap::new(), constant: Some(constant) }
    }

    /// `self operator other`. `None` if both have terms and `operator` is neither adding nor subtracting.
    fn combine(mut self, operator: &T::Operator, other: Self) -> Option<Self> {
        if self.terms.is_empty() && other.terms.is_empty() {
            let (a, b) = (self.constant?, other.constant?);
            return Some(Self::constant(T::operation(&a, operator, &b)));
        }
        let arithmetic = T::linear_arithmetic()?;
        let sign = match operator {
            operator if *operator == arithmetic.add => 1,
            operator if *operator == arithmetic.subtract => -1,
            _ => return None,
        };
        for (param, coefficient) in other.terms {
            let sum = self.terms.get(&param).copied().unwrap_or(0).checked_add(coefficient.checked_mul(sign)?)?;
            self.terms.insert(param, sum);
        }
        self.terms.retain(|_, coefficient| *coefficient != 0);
        self.constant = match (self.constant, other.constant) {
            (Some(a), Some(b)) => Some(T::operation(&a, operator, &b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) if sign == 1 => Some(b),
            (None, Some(b)) => Some(T::operation(&arithmetic.zero, operator, &b)),
            (None, None) => None,
        };
        Some(self)
    }
}

impl<T: Type, R: AsScopedRef<T>> TypeExpr<T, R> {
    /// The linear form of `self` if it is built from rigid parameters and concrete types with the operators of
    /// [Type::linear_arithmetic].
    ///
    /// # Returns
    /// `None` if `self` references an uninferred parameter or has any other shape.
    pub fn linear_form(&self, scope: &ScopePointer<T>) -> Option<LinearForm<T>> {
        match self {
            Self::Operation { a, operator, b } => a.linear_form(scope)?.combine(operator, b.linear_form(scope)?),
            Self::Ref(r) => match r.view() {
                ScopedRefView::Param(param) => {
                    let (var, param_scope) = scope.lookup(&param.param_id)?;
                    if let Some((inferred, inferred_scope)) = var.inferred() {
                        return inferred.linear_form(&inferred_scope);
                    }
                    var.is_rigid().then(|| LinearForm {
                        terms: [(
                            GlobalParameterId { scope: ScopePointer::clone(param_scope), local_id: param.param_id },
                            1,
                        )]
                        .into(),
                        constant: None,
                    })
                }
                ScopedRefView::ScopedExpr { expr, scope } => expr.linear_form(scope),
            },
            other => Some(LinearForm::constant(other.normalize_concrete(scope)?)),
        }
    }

    /// The linear form of `self` if it has rigid terms, so that it compares as a parent only to linear forms.
    /// Only a rigid parameter itself or an operation can have one.
    pub(crate) fn rigid_linear_form(&self, scope: &ScopePointer<T>) -> Option<LinearForm<T>> {
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
        self.linear_form(scope).filter(|form| !form.terms.is_empty())
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
