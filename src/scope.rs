use crate::{
    scope::type_parameter::TypeParameter,
    r#type::Type,
    type_expr::{ScopePortal, ScopedTypeExpr, TypeExpr},
};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    ops::Deref,
    rc::Rc,
};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

pub mod type_parameter;

/// Identifies a type parameter within a type expression or node signature.
/// Can be created from a single character (e.g. `'T'`) or a string (hashed for multi-char names).
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct LocalParamID(pub u32);

impl From<char> for LocalParamID {
    fn from(c: char) -> Self {
        LocalParamID(c as u32)
    }
}

impl From<&str> for LocalParamID {
    fn from(str: &str) -> Self {
        if str.len() == 1 {
            Self(str.as_bytes()[0] as u32)
        } else {
            let mut hasher = DefaultHasher::new();
            str.hash(&mut hasher);
            let key_hash = hasher.finish();
            Self(key_hash as u32)
        }
    }
}

#[derive(Debug)]
pub enum ScopeError {
    ParameterNotFound,
    ParameterAlreadyInferred,
}

#[derive(Debug, Clone)]
pub struct GlobalParameterId<T: Type> {
    pub scope: ScopePointer<T>,
    pub local_id: LocalParamID,
}

// Custom impl to not have the bound on T
impl<T: Type> PartialEq for GlobalParameterId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.scope == other.scope && self.local_id == other.local_id
    }
}

// Custom impl to not have the bound on T
impl<T: Type> Eq for GlobalParameterId<T> {}

// Custom impl to not have the bound on T
impl<T: Type> Hash for GlobalParameterId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scope.hash(state);
        self.local_id.hash(state);
    }
}

/// Reference-counted pointer to a [`Scope`].
///
/// **Equality is by pointer identity**, not structural comparison.
/// Two `ScopePointer`s are equal only if they point to the exact same
/// allocation.
#[derive(Debug, Clone)]
pub struct ScopePointer<T: Type>(Rc<Scope<T>>);

impl<T: Type> Deref for ScopePointer<T> {
    type Target = Scope<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Type> Default for ScopePointer<T> {
    fn default() -> Self {
        Self(Rc::new(Scope::new_root()))
    }
}

/// Pointer identity — see [`ScopePointer`] docs.
impl<T: Type> PartialEq for ScopePointer<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: Type> Eq for ScopePointer<T> {}

/// Consistent with [`PartialEq`] — hashes the pointer address.
impl<T: Type> Hash for ScopePointer<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

impl<T: Type> ScopePointer<T> {
    pub fn new(scope: Scope<T>) -> Self {
        Self(Rc::new(scope))
    }

    pub fn new_root() -> Self {
        Self(Rc::new(Scope::new_root()))
    }
}

/// Type scope in the tree of types.
/// Scopes are additive.
/// - What was once defined cannot get undefined.
/// - What was once inferred cannot get uninferred.
#[derive(Debug, Clone)]
pub struct Scope<T: Type> {
    variables: HashMap<LocalParamID, RegisteredTypeVar<T>>,
    parent: Option<ScopePointer<T>>,
}

/// A type parameter in a scope with its inferred value (if any).
#[derive(Debug, Clone)]
pub struct RegisteredTypeVar<T: Type> {
    parameter: TypeParameter<T, ScopePortal<T>>,
    inferred: RefCell<Option<(ScopedTypeExpr<T>, ScopePointer<T>)>>,
}

impl<T: Type> RegisteredTypeVar<T> {
    /// # Arguments
    /// - `scope` (`&ScopePointer<T>`) - The scope in which this parameter is defined.
    pub fn get_boundary<'a>(&'a self, scope: &ScopePointer<T>) -> (Cow<'a, ScopedTypeExpr<T>>, ScopePointer<T>) {
        if let Some((inferred, inferred_scope)) = self.inferred.borrow().clone() {
            (Cow::Owned(inferred), inferred_scope)
        } else if let Some(bound) = &self.parameter.bound {
            (Cow::Borrowed(bound), ScopePointer::clone(scope))
        } else {
            (Cow::Owned(TypeExpr::Any), ScopePointer::clone(scope))
        }
    }

    pub fn parameter(&self) -> &TypeParameter<T, ScopePortal<T>> {
        &self.parameter
    }

    pub fn inferred(&self) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        self.inferred.borrow().clone()
    }

    pub fn is_inferred(&self) -> bool {
        self.inferred.borrow().is_some()
    }
}

impl<T: Type> ScopePointer<T> {
    /// Looks up a parameter by local id
    ///
    /// # Returns
    /// (parameter, the defining scope)
    pub fn lookup<'b>(&'b self, parameter: &LocalParamID) -> Option<(&'b RegisteredTypeVar<T>, &'b ScopePointer<T>)> {
        if let Some(local_type) = self.0.variables.get(parameter) {
            Some((local_type, self))
        } else if let Some(parent) = &self.parent {
            parent.lookup(parameter)
        } else {
            None
        }
    }

    pub fn lookup_global<'b>(
        &'b self,
        global_id: &GlobalParameterId<T>,
    ) -> Option<(&'b RegisteredTypeVar<T>, &'b ScopePointer<T>)> {
        if self == &global_id.scope {
            self.lookup(&global_id.local_id)
        } else if let Some(parent) = &self.parent {
            parent.lookup_global(global_id)
        } else {
            None
        }
    }

    /// Returns the inferred type and scope for a parameter, or `None` if not inferred.
    pub fn lookup_inferred(&self, parameter: &LocalParamID) -> Option<(ScopedTypeExpr<T>, ScopePointer<T>)> {
        let (RegisteredTypeVar { inferred: ref_cell, .. }, _) = self.lookup(parameter)?;
        let (inferred, inferred_scope) = ref_cell.borrow().clone()?;
        Some((inferred, inferred_scope))
    }

    /// Returns the effective bound (inferred, bound or Any) for the parameter.
    pub fn lookup_bound<'b>(
        &'b self,
        parameter: &LocalParamID,
    ) -> Option<(Cow<'b, ScopedTypeExpr<T>>, ScopePointer<T>)> {
        let (var, scope) = self.lookup(parameter)?;
        Some(var.get_boundary(scope))
    }

    /// Sets defaults for all uninferred parameters.
    /// Uses defaults in the following order:
    /// 1. Param default
    /// 2. Param bound
    /// 3. Any
    pub fn infer_defaults(&self) {
        let uninferred = self.uninferred().map(|(gid, _param)| gid).collect::<Vec<_>>();
        for param_id in uninferred {
            let var = self.variables.get(&param_id).unwrap();
            let default = if let Some(default) = var.parameter.default.clone() {
                default
            } else if let Some(bound) = var.parameter.bound.clone() {
                bound
            } else {
                TypeExpr::Any
            };
            self.infer(&param_id, default, ScopePointer::clone(self)).expect("expected var not to be inferred yet");
        }
    }

    pub fn lookup_scope(&self, parameter: &LocalParamID) -> Option<ScopePointer<T>> {
        if self.0.variables.contains_key(parameter) {
            return Some(ScopePointer::clone(self));
        }
        if let Some(parent) = &self.parent { parent.lookup_scope(parameter) } else { None }
    }
}

impl<T: Type> Scope<T> {
    /// Creates a root scope with no parent.
    pub fn new_root() -> Self {
        Self { variables: HashMap::new(), parent: None }
    }

    /// Creates a child scope that inherits from `parent`.
    pub fn new_child(parent: &ScopePointer<T>) -> Self {
        // if parent.is_inferred(&LocalParamID(0)) {
        //     panic!("parent scope is inferred");
        // }
        Self { variables: HashMap::new(), parent: Some(ScopePointer::clone(parent)) }
    }

    /// Defines a type parameter in this scope.
    pub fn define(&mut self, ident: LocalParamID, parameter: TypeParameter<T, ScopePortal<T>>) {
        self.variables.insert(ident, RegisteredTypeVar { parameter, inferred: RefCell::new(None) });
    }

    /// Sets the inferred type for a parameter (no-op if already inferred).
    ///
    /// It is the callers responsibility to not create cyclic references.
    pub fn infer(
        &self,
        ident: &LocalParamID,
        inferred: TypeExpr<T, ScopePortal<T>>,
        inferred_scope: ScopePointer<T>,
    ) -> Result<(), ScopeError> {
        let Some(registered) = self.variables.get(ident) else {
            return Err(ScopeError::ParameterNotFound);
        };
        let mut inferred_ref = registered.inferred.borrow_mut();
        if inferred_ref.is_some() {
            return Err(ScopeError::ParameterAlreadyInferred);
        }
        *inferred_ref = Some((inferred, inferred_scope));
        Ok(())
    }

    /// # Returns
    /// An iterator over all variable identifiers that are not yet inferred.
    pub fn uninferred(&self) -> impl Iterator<Item = (LocalParamID, TypeParameter<T, ScopePortal<T>>)> {
        self.variables.iter().filter_map(|(ident, param)| {
            if param.inferred.borrow().is_none() { Some((*ident, param.parameter.clone())) } else { None }
        })
    }

    pub fn variables(&self) -> &HashMap<LocalParamID, RegisteredTypeVar<T>> {
        &self.variables
    }

    /// Total number of parameters defined in this scope and all ancestors.
    pub fn count_defined(&self) -> usize {
        if let Some(parent) = &self.parent {
            parent.count_defined() + self.variables.len()
        } else {
            self.variables.len()
        }
    }

    pub fn is_inferred(&self, ident: &LocalParamID) -> bool {
        if let Some(local_var) = self.variables.get(ident) {
            local_var.is_inferred()
        } else if let Some(parent) = &self.parent {
            parent.is_inferred(ident)
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.variables.is_empty() && self.parent.as_ref().is_none_or(|parent| parent.is_empty())
    }

    /// Returns an iterator over all defined variables in this scope and its parents.
    /// Variables from the current scope are yielded first, then the parent's, and so on.
    pub fn all_defined(&self) -> Box<dyn Iterator<Item = (LocalParamID, &RegisteredTypeVar<T>)> + '_> {
        let self_iter = self.variables.iter().map(|(id, var)| (*id, var));
        let parent_iter = self.parent.as_ref().map(|p| p.all_defined()).into_iter().flatten();
        Box::new(self_iter.chain(parent_iter))
    }

    pub fn parent(&self) -> &Option<ScopePointer<T>> {
        &self.parent
    }
}
