use crate::common::{expr_u, sig_u};
use nodety::{
    NodeSignature,
    demo_type::DemoType,
    scope::LocalParamID,
    type_expr::{TypeExpr, Unscoped},
};
use std::str::FromStr;

mod common;

#[test]
fn test_format_basic_types() {
    assert_eq!("Integer", format!("{}", expr_u("Integer")));
    assert_eq!("String", format!("{}", expr_u("String")));
    assert_eq!("Boolean", format!("{}", expr_u("Boolean")));
    assert_eq!("Float", format!("{}", expr_u("Float")));
    assert_eq!("Any", format!("{}", expr_u("Any")));
    assert_eq!("Never", format!("{}", expr_u("Never")));
    assert_eq!("Unit", format!("{}", expr_u("Unit")));
    assert_eq!("Comparable", format!("{}", expr_u("Comparable")));
    assert_eq!("Countable", format!("{}", expr_u("Countable")));
    assert_eq!("Sortable", format!("{}", expr_u("Sortable")));
    assert_eq!("AnySI", format!("{}", expr_u("AnySI")));
}

#[test]
fn test_format_string_literal() {
    assert_eq!("\"hello\"", format!("{}", expr_u("'hello'")));
}

#[test]
fn test_format_union() {
    assert_eq!("Integer | String", format!("{}", expr_u("Integer | String")));
}

#[test]
fn test_format_intersection() {
    let e: TypeExpr<DemoType, Unscoped> =
        TypeExpr::Intersection(Box::new(expr_u("{a: Integer}")), Box::new(expr_u("{b: String}")));
    let formatted = format!("{}", e);
    assert!(formatted.contains(" & "), "Expected '&' in: {formatted}");
}

#[test]
fn test_format_keyof() {
    assert_eq!("keyof {a: Integer}", format!("{}", expr_u("keyof {a: Integer}")));
}

#[test]
fn test_format_index() {
    assert_eq!("{a: Integer}[\"a\"]", format!("{}", expr_u("{a: Integer}['a']")));
}

#[test]
fn test_format_conditional() {
    let formatted = format!("{}", expr_u("Integer extends Comparable ? Integer : Never"));
    assert!(formatted.contains("extends"), "Expected 'extends' in: {formatted}");
    assert!(formatted.contains("?"), "Expected '?' in: {formatted}");
    assert!(formatted.contains(":"), "Expected ':' in: {formatted}");
}

#[test]
fn test_format_operation() {
    assert_eq!("SI(1, 0, 1) * SI(1, 1)", format!("{}", expr_u("SI(1,0,1) * SI(1,1)")));
    assert_eq!("SI(1, 0, 1) / SI(1, 1)", format!("{}", expr_u("SI(1,0,1) / SI(1,1)")));
}

#[test]
fn test_format_array_with_param() {
    assert_eq!("Array<Integer>", format!("{}", expr_u("Array<Integer>")));
}

#[test]
fn test_format_record() {
    assert_eq!("{a: Integer}", format!("{}", expr_u("{a: Integer}")));
    assert_eq!("{a: Integer, b: String}", format!("{}", expr_u("{a: Integer, b: String}")));
}

#[test]
fn test_format_empty_record() {
    assert_eq!("{}", format!("{}", expr_u("{}")));
}

#[test]
fn test_format_node_signature() {
    let sig = NodeSignature::<DemoType>::from_str("<T>(T) -> (T)").unwrap();
    assert_eq!("<T>(T) -> (T)", format!("{}", sig));
}

#[test]
fn test_format_node_signature_no_params() {
    let sig = NodeSignature::<DemoType>::from_str("(Integer) -> (String)").unwrap();
    assert_eq!("(Integer) -> (String)", format!("{}", sig));
}

#[test]
fn test_format_node_signature_with_bound() {
    let sig = NodeSignature::<DemoType>::from_str("<T extends Comparable>(T) -> (T)").unwrap();
    assert_eq!("<T extends Comparable>(T) -> (T)", format!("{}", sig));
}

#[test]
fn test_format_node_signature_with_default() {
    let sig = NodeSignature::<DemoType>::from_str("<T = Integer>(T) -> (T)").unwrap();
    assert_eq!("<T = Integer>(T) -> (T)", format!("{}", sig));
}

#[test]
fn test_format_port_types_with_varg() {
    assert_eq!("(Integer, ...String) -> ()", format!("{}", sig_u("(Integer, ...String) -> ()")));
}

#[test]
fn test_format_port_types_empty() {
    assert_eq!("() -> ()", format!("{}", sig_u("() -> ()")));
}

#[test]
fn test_format_type_parameter_with_infer_false() {
    let e: TypeExpr<DemoType, Unscoped> = TypeExpr::TypeParameter(LocalParamID::from('T'), false);
    assert_eq!("!T", format!("{}", e));
}

#[test]
fn test_format_type_parameter_numeric_id() {
    let e: TypeExpr<DemoType, Unscoped> = TypeExpr::TypeParameter(LocalParamID(0), true);
    assert_eq!("#0", format!("{}", e));
}

#[test]
fn test_format_si_with_all_components() {
    assert_eq!("SI(1, 1, 2, 3)", format!("{}", expr_u("SI(1,1,2,3)")));
}

#[test]
fn test_format_roundtrip_complex_signature() {
    let input = "<T, U>(Array<T>, (T) -> (U)) -> (Array<U>)";
    let sig = NodeSignature::<DemoType>::from_str(input).unwrap();
    assert_eq!(input, format!("{}", sig));
}

#[test]
fn test_format_nested_node_signature_expr() {
    assert_eq!("(Integer) -> (String)", format!("{}", expr_u("(Integer) -> (String)")));
}

#[test]
fn test_format_string_utility() {
    use nodety::notation::format::format_string;
    assert_eq!("hello", format_string("hello"));
    assert_eq!("\"\"", format_string(""));
    assert_eq!("\"hello world\"", format_string("hello world"));
    assert_eq!("\"hello\\\"world\"", format_string("hello\"world"));
}
