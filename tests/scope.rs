use crate::common::expr;
use nodety::{
    demo_type::DemoType,
    scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    type_expr::TypeExpr,
};

mod common;

#[test]
fn test_scope_count_defined_nested() {
    let mut root = Scope::<DemoType>::new_root();
    root.define(LocalParamID(0), TypeParameter::default());
    let root_ptr = ScopePointer::new(root);

    let mut child = Scope::new_child(&root_ptr);
    child.define(LocalParamID(1), TypeParameter::default());
    let child_ptr = ScopePointer::new(child);

    assert_eq!(child_ptr.count_defined(), 2);
    assert_eq!(root_ptr.count_defined(), 1);
}

#[test]
fn test_scope_is_empty() {
    let root = Scope::<DemoType>::new_root();
    assert!(root.is_empty());

    let ptr = ScopePointer::new(root);
    let child = Scope::new_child(&ptr);
    assert!(child.is_empty());

    let mut non_empty = Scope::<DemoType>::new_root();
    non_empty.define(LocalParamID(0), TypeParameter::default());
    assert!(!non_empty.is_empty());
}

#[test]
fn test_scope_is_empty_nested_non_empty_parent() {
    let mut root = Scope::<DemoType>::new_root();
    root.define(LocalParamID(0), TypeParameter::default());
    let root_ptr = ScopePointer::new(root);

    let empty_child = Scope::new_child(&root_ptr);
    assert!(!empty_child.is_empty());
}

#[test]
fn test_scope_all_defined() {
    let mut root = Scope::<DemoType>::new_root();
    root.define(LocalParamID(0), TypeParameter::default());
    let root_ptr = ScopePointer::new(root);

    let mut child = Scope::new_child(&root_ptr);
    child.define(LocalParamID(1), TypeParameter::default());

    let all: Vec<_> = child.all_defined().collect();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_scope_infer_already_inferred_error() {
    let mut scope = Scope::<DemoType>::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());

    scope.infer(&LocalParamID(0), expr("Integer"), ScopePointer::new_root()).unwrap();
    let result = scope.infer(&LocalParamID(0), expr("String"), ScopePointer::new_root());
    assert!(result.is_err());
}

#[test]
fn test_scope_infer_parameter_not_found_error() {
    let scope = Scope::<DemoType>::new_root();
    let result = scope.infer(&LocalParamID(99), expr("Integer"), ScopePointer::new_root());
    assert!(result.is_err());
}

#[test]
fn test_scope_infer_defaults_with_default() {
    let mut scope = Scope::<DemoType>::new_root();
    scope.define(LocalParamID(0), TypeParameter { bound: None, default: Some(expr("Integer")) });
    let scope_ptr = ScopePointer::new(scope);
    scope_ptr.infer_defaults();

    let (inferred, _) = scope_ptr.lookup_inferred(&LocalParamID(0)).unwrap();
    assert_eq!(inferred, expr("Integer"));
}

#[test]
fn test_scope_infer_defaults_with_bound_no_default() {
    let mut scope = Scope::<DemoType>::new_root();
    scope.define(LocalParamID(0), TypeParameter { bound: Some(expr("Comparable")), default: None });
    let scope_ptr = ScopePointer::new(scope);
    scope_ptr.infer_defaults();

    let (inferred, _) = scope_ptr.lookup_inferred(&LocalParamID(0)).unwrap();
    assert_eq!(inferred, expr("Comparable"));
}

#[test]
fn test_scope_infer_defaults_no_bound_no_default() {
    let mut scope = Scope::<DemoType>::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    let scope_ptr = ScopePointer::new(scope);
    scope_ptr.infer_defaults();

    let (inferred, _) = scope_ptr.lookup_inferred(&LocalParamID(0)).unwrap();
    assert_eq!(inferred, TypeExpr::Any);
}

#[test]
fn test_scope_lookup_child_sees_parent() {
    let mut root = Scope::<DemoType>::new_root();
    root.define(LocalParamID(0), TypeParameter::default());
    let root_ptr = ScopePointer::new(root);

    let child = Scope::new_child(&root_ptr);
    let child_ptr = ScopePointer::new(child);

    assert!(child_ptr.lookup(&LocalParamID(0)).is_some());
    assert!(child_ptr.lookup(&LocalParamID(99)).is_none());
}

#[test]
fn test_scope_is_inferred() {
    let mut scope = Scope::<DemoType>::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    assert!(!scope.is_inferred(&LocalParamID(0)));
    assert!(!scope.is_inferred(&LocalParamID(99)));

    scope.infer(&LocalParamID(0), expr("Integer"), ScopePointer::new_root()).unwrap();
    assert!(scope.is_inferred(&LocalParamID(0)));
}

#[test]
fn test_scope_lookup_scope() {
    let mut root = Scope::<DemoType>::new_root();
    root.define(LocalParamID(0), TypeParameter::default());
    let root_ptr = ScopePointer::new(root);

    assert!(root_ptr.lookup_scope(&LocalParamID(0)).is_some());
    assert!(root_ptr.lookup_scope(&LocalParamID(99)).is_none());
}

#[test]
fn test_local_param_id_from_char() {
    let id = LocalParamID::from('T');
    assert_eq!(id, LocalParamID('T' as u32));
}

#[test]
fn test_local_param_id_from_multichar_str() {
    let id1 = LocalParamID::from("hello");
    let id2 = LocalParamID::from("hello");
    assert_eq!(id1, id2);

    let id3 = LocalParamID::from("world");
    assert_ne!(id1, id3);
}
