use crate::common::{expr, graph, sig_u};
use nodety::{
    Edge, NoOperator, Nodety, Type,
    inference::InferenceConfig,
    notation::parse::{ParsableType, parse_record},
    scope::{LocalParamID, Scope, ScopePointer, type_parameter::TypeParameter},
    type_expr::{ParamTypeRef, ScopedTypeExpr, TypeExpr, node_signature::NodeSignature},
    validation::{GraphLocation, ValidationError, ValidationErrorKind},
};
use nom::{IResult, Parser, branch::alt, bytes::complete::tag, combinator::value};
use std::str::FromStr;

mod common;

/// A type system whose `Any` includes `Unit`, so `Any` is optional in constructors.
#[derive(Debug, Clone, PartialEq)]
enum UnitInAny {
    Integer,
    Unit,
    Record,
}

impl Type for UnitInAny {
    type Operator = NoOperator;

    fn optional_in_constructor(&self) -> bool {
        matches!(self, Self::Unit)
    }

    fn any_optional_in_constructor() -> bool {
        true
    }
}

impl ParsableType for UnitInAny {
    fn parse<R: ParamTypeRef>(input: &str) -> IResult<&str, TypeExpr<Self, R>> {
        alt((
            value(TypeExpr::Type(Self::Integer), tag("Integer")),
            value(TypeExpr::Type(Self::Unit), tag("Unit")),
            parse_record.map(|parameters| TypeExpr::Constructor { inner: Self::Record, parameters }),
        ))
        .parse(input)
    }
}

fn unit_in_any_graph(nodes: &[&str], edges: &[(usize, usize, usize, usize)]) -> Nodety<UnitInAny> {
    let mut nodety = Nodety::new();
    let ids: Vec<_> = nodes.iter().map(|n| nodety.add_node(NodeSignature::from_str(n).unwrap()).unwrap()).collect();
    for &(source, target, source_port, target_port) in edges {
        nodety.add_edge(ids[source], ids[target], Edge { source_port, target_port }).unwrap();
    }
    nodety
}

fn unit_in_any_expr(input: &str) -> ScopedTypeExpr<UnitInAny> {
    TypeExpr::from_str(input).unwrap()
}

fn missing_inputs<T: Type>(nodety: &Nodety<T>) -> Vec<usize> {
    nodety
        .validate(&nodety.infer(&InferenceConfig::default()))
        .into_iter()
        .filter_map(|e| match e {
            ValidationError {
                location: GraphLocation::InputPort { input_idx, .. },
                kind: ValidationErrorKind::EdgeMissingOnInput,
            } => Some(input_idx),
            _ => None,
        })
        .collect()
}

#[test]
fn any_field_is_optional_iff_the_type_says_so() {
    assert!(unit_in_any_expr("{a: Any}").supertype_of_naive(&unit_in_any_expr("{}")).is_supertype());
    assert!(!unit_in_any_expr("{a: Integer}").supertype_of_naive(&unit_in_any_expr("{}")).is_supertype());
    assert!(!expr("{a: Any}").supertype_of_naive(&expr("{}")).is_supertype());
}

#[test]
fn generic_field_stays_required_until_inferred() {
    let mut scope = Scope::new_root();
    scope.define(LocalParamID(0), TypeParameter::default());
    let scope = ScopePointer::new(scope);
    let result = unit_in_any_expr("{a: #0}").supertype_of(&unit_in_any_expr("{}"), &scope, &scope);
    assert!(!result.is_supertype());
}

#[test]
fn optional_inputs_need_no_edge() {
    assert_eq!(missing_inputs(&graph(vec![sig_u("(Integer | Unit, Unit) -> ()")], vec![])), Vec::<usize>::new());
    assert_eq!(missing_inputs(&graph(vec![sig_u("(Integer, Any) -> ()")], vec![])), vec![0, 1]);
    assert_eq!(missing_inputs(&unit_in_any_graph(&["(Integer, Any, Integer | Unit) -> ()"], &[])), vec![0]);
}

#[test]
fn uninferred_type_parameter_inputs_stay_required() {
    assert_eq!(missing_inputs(&unit_in_any_graph(&["<T>(T) -> ()"], &[])), vec![0]);
    assert_eq!(missing_inputs(&unit_in_any_graph(&["<T extends Any>(T) -> ()"], &[])), vec![0]);
}

#[test]
fn inferred_optional_type_parameter_input_needs_no_edge() {
    let nodety = unit_in_any_graph(&["() -> (Integer | Unit)", "<T>(T, T) -> ()"], &[(0, 1, 0, 0)]);
    assert_eq!(missing_inputs(&nodety), Vec::<usize>::new());
}

#[test]
fn optional_variadic_inputs_may_be_skipped() {
    let nodety = unit_in_any_graph(&["() -> (Integer)", "(...Any) -> ()"], &[(0, 1, 0, 2)]);
    assert_eq!(missing_inputs(&nodety), Vec::<usize>::new());
    let nodety = unit_in_any_graph(&["() -> (Integer)", "(...Integer) -> ()"], &[(0, 1, 0, 2)]);
    assert_eq!(missing_inputs(&nodety), vec![0, 1]);
}
