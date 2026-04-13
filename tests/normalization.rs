use crate::common::expr;
use nodety::{
    demo_type::DemoType,
    scope::{Scope, ScopePointer},
};
mod common;

#[test]
fn test_normalization_of_index_and_type_param() {
    let scope = Scope::<DemoType>::try_parse("<T>").unwrap();
    scope.infer(&"T".into(), expr("{a: Integer}"), ScopePointer::new_root()).unwrap();

    let normalized = expr("T['a']").normalize(&ScopePointer::new(scope));

    assert_eq!(normalized, expr("Integer"));
}

#[test]
fn test_normalize_conditional() {
    let scope = Scope::<DemoType>::try_parse("<T>").unwrap();
    scope.infer(&"T".into(), expr("Integer | Unit"), ScopePointer::new_root()).unwrap();

    let normalized = expr("T extends Unit ? Never : T").normalize(&ScopePointer::new(scope));

    assert_eq!(normalized, expr("Integer"));
}
