use crate::common::{expr, sig, sig_u};
use assert_matches::assert_matches;
use maplit::btreemap;
use nodety::demo_type::{DemoType, SIUnit};
use nodety::notation::parse::{
    parse_quoted_string, parse_si_unit, parse_type_expr, parse_type_expr_union, parse_type_parameter,
    parse_type_parameter_declarations,
};
use nodety::scope::{LocalParamID, Scope};
use nodety::type_expr::{
    ParamRef, ScopedTypeRef, TypeExpr,
    conditional::Conditional,
    node_signature::{NodeSignature, port_types::PortTypes, type_parameters::TypeParameters},
};
use std::collections::BTreeSet;

mod common;

#[test]
fn test_union_nested() {
    assert_eq!(
        parse_type_expr_union::<DemoType, ParamRef>("Boolean|Float|String"),
        Ok((
            "",
            TypeExpr::Union(
                Box::new(TypeExpr::Union(
                    Box::new(TypeExpr::Type(DemoType::Boolean)),
                    Box::new(TypeExpr::Type(DemoType::Float)),
                )),
                Box::new(TypeExpr::Type(DemoType::String(None)))
            )
        ))
    );
}

#[test]
fn test_node_signature() {
    let signature = sig("() -> ()");
    assert_eq!(NodeSignature::default(), signature);

    let signature = sig("(a: Integer) -> (b: String)");
    assert_eq!(
        NodeSignature {
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Type(DemoType::Integer)]))),
            outputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Type(DemoType::String(None))]))),
            ..Default::default()
        },
        signature
    );
}

#[test]
fn test_node_signature_with_defaults() {
    let signature = sig("() -> ()");
    assert_eq!(NodeSignature::default(), signature);

    let signature = sig("(Integer = Integer) -> (String)");
    assert_eq!(
        NodeSignature {
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Type(DemoType::Integer)]))),
            outputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Type(DemoType::String(None))]))),
            default_input_types: btreemap! {0 => TypeExpr::Type(DemoType::Integer)},
            ..Default::default()
        },
        signature
    );
}

#[test]
fn test_node_signature_primitive_arr() {
    let signature = sig("(Array<Integer>) -> ()");
    assert_eq!(
        NodeSignature {
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Constructor {
                inner: DemoType::Array,
                parameters: btreemap! {"elements_type".to_string() => TypeExpr::Type(DemoType::Integer)}.into()
            }]))),
            ..Default::default()
        },
        signature
    );
}

#[test]
fn test_generic_array() {
    let signature = sig("<T>(Array<T>) -> ()");
    assert_eq!(
        NodeSignature::<DemoType, ScopedTypeRef<DemoType>>::from(NodeSignature {
            parameters: parse_type_parameter_declarations::<DemoType, ParamRef>("<T>").unwrap().1,
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Constructor {
                inner: DemoType::Array,
                parameters: [("elements_type".into(), TypeExpr::param_with_infer(LocalParamID::from("T"), true))]
                    .into_iter()
                    .collect()
            }]))),
            ..Default::default()
        }),
        signature
    );
}

#[test]
fn test_sig_with_union() {
    let signature = sig("(Integer|String) -> ()");
    assert_eq!(
        NodeSignature {
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Union(
                Box::new(TypeExpr::Type(DemoType::Integer)),
                Box::new(TypeExpr::Type(DemoType::String(None)))
            )]))),
            ..Default::default()
        },
        signature
    );
}

#[test]
fn test_sig_with_generic_union() {
    let signature = sig("<T>(Integer|T) -> ()");
    assert_eq!(
        NodeSignature::<DemoType, ScopedTypeRef<DemoType>>::from(NodeSignature {
            parameters: parse_type_parameter_declarations::<DemoType, ParamRef>("<T>").unwrap().1,
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Union(
                Box::new(TypeExpr::Type(DemoType::Integer)),
                Box::new(TypeExpr::param_with_infer(LocalParamID::from("T"), true))
            )]))),
            ..Default::default()
        }),
        signature
    );
}

#[test]
fn test_keyof() {
    let signature = sig("<T>(keyof T) -> ()");
    assert_eq!(
        NodeSignature::<DemoType, ScopedTypeRef<DemoType>>::from(NodeSignature {
            parameters: parse_type_parameter_declarations::<DemoType, ParamRef>("<T>").unwrap().1,
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::KeyOf(Box::new(
                TypeExpr::param_with_infer(LocalParamID::from("T"), true)
            ))]))),
            ..Default::default()
        }),
        signature
    );
}

#[test]
fn test_string_literal() {
    let signature = sig("(\"A\") -> ()");
    assert_eq!(
        NodeSignature::<DemoType, ScopedTypeRef<DemoType>>::from(NodeSignature {
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::<DemoType, ParamRef>::Type(
                DemoType::String(Some("A".into()))
            )]))),
            ..Default::default()
        }),
        signature
    );
}

#[test]
fn test_param_with_idx() {
    assert_eq!(LocalParamID(0), parse_type_parameter("#0").unwrap().1.0);

    let signature = sig("<#0>(#0) -> ()");
    assert_eq!(
        NodeSignature::<DemoType, ScopedTypeRef<DemoType>>::from(NodeSignature {
            parameters: parse_type_parameter_declarations::<DemoType, ParamRef>("<#0>").unwrap().1,
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::param_with_infer(
                LocalParamID(0),
                true
            )]))),
            ..Default::default()
        }),
        signature
    );
}

#[test]
fn test_index() {
    assert_eq!(LocalParamID(0), parse_type_parameter("#0").unwrap().1.0);

    let signature = sig("({}[\"a\"]) -> ()");
    assert_eq!(
        NodeSignature {
            inputs: TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Index {
                expr: Box::new(TypeExpr::Constructor { inner: DemoType::Record, parameters: Default::default() }),
                index: Box::new(TypeExpr::Type(DemoType::String(Some("a".into()))))
            }]))),
            ..Default::default()
        },
        signature
    );
}

#[test]
fn test_index_chained() {
    let parsed = TypeExpr::<DemoType, ParamRef>::try_parse("Integer['a']['b']").unwrap();

    // Nests left to right: `(Integer['a'])['b']`.
    let expected = TypeExpr::Index {
        expr: Box::new(TypeExpr::Index {
            expr: Box::new(DemoType::Integer.into()),
            index: Box::new(TypeExpr::Type(DemoType::String(Some("a".into())))),
        }),
        index: Box::new(TypeExpr::Type(DemoType::String(Some("b".into())))),
    };

    assert_eq!(expected, parsed);

    // The formatter parenthesises the inner access, which reparses to the same tree.
    assert_eq!(parsed, TypeExpr::<DemoType, ParamRef>::try_parse(&format!("{parsed}")).unwrap());
}

#[test]
fn test_intersection() {
    let expr = parse_type_expr::<DemoType, ParamRef>("Integer & String").unwrap().1;
    assert_eq!(
        TypeExpr::Intersection(
            Box::new(TypeExpr::Type(DemoType::Integer)),
            Box::new(TypeExpr::Type(DemoType::String(None)))
        ),
        expr
    );
}

#[test]
fn test_intersection_nested_brackets() {
    let expr = parse_type_expr::<DemoType, ParamRef>("Integer & (String|Float)").unwrap().1;
    assert_eq!(
        TypeExpr::Intersection(
            Box::new(TypeExpr::Type(DemoType::Integer)),
            Box::new(TypeExpr::Union(
                Box::new(TypeExpr::Type(DemoType::String(None))),
                Box::new(TypeExpr::Type(DemoType::Float)),
            ))
        ),
        expr
    );
}

#[test]
fn test_conditional() {
    let expr = parse_type_expr::<DemoType, ParamRef>("Integer extends String ? Boolean : Float").unwrap().1;
    assert_eq!(
        TypeExpr::Conditional(Box::new(Conditional {
            t_test: TypeExpr::Type(DemoType::Integer),
            t_test_bound: TypeExpr::Type(DemoType::String(None)),
            t_then: TypeExpr::Type(DemoType::Boolean),
            t_else: TypeExpr::Type(DemoType::Float),
            infer: BTreeSet::new(),
        })),
        expr
    );
}

#[test]
fn test_conditional_union() {
    let expr = expr("(Integer|String) extends String ? Boolean : Float");
    assert_eq!(
        TypeExpr::Conditional(Box::new(Conditional {
            t_test: TypeExpr::Union(
                Box::new(TypeExpr::Type(DemoType::Integer)),
                Box::new(TypeExpr::Type(DemoType::String(None)))
            ),
            t_test_bound: TypeExpr::Type(DemoType::String(None)),
            t_then: TypeExpr::Type(DemoType::Boolean),
            t_else: TypeExpr::Type(DemoType::Float),
            infer: BTreeSet::new(),
        })),
        expr
    );
}

#[test]
fn test_failed_to_parse() {
    expr("() -> (...Any)");
}

#[test]
fn test_parse_quoted_string() {
    assert_eq!(("Rest", r#"abc"de fg"#.to_string()), parse_quoted_string(r#""abc\"de fg"Rest"#).unwrap());
    assert_eq!(("Rest", "abc".to_string()), parse_quoted_string("'abc'Rest").unwrap());
}

#[test]
fn test_parse_large_union() {
    (expr(
        "({ property: keyof A | 'place', direction: 'AscRequired' | 'DescRequired' | 'AscNoneFirst' | 'AscNoneLast' | 'DescNoneFirst' | 'DescNoneLast' }) -> ()",
    ));
}

#[test]
fn test_parse_si_unit() {
    let (unit, scale) = parse_si_unit("SI(1, 1, 2, 3, 4, 5, 6, 7)").unwrap().1;
    assert_eq!(SIUnit { s: 1, m: 2, kg: 3, a: 4, k: 5, mol: 6, cd: 7 }, unit);
    assert_eq!(1.0, scale);

    let (unit, scale) = parse_si_unit("SI(1, 1, 0, 0, 0, 0, 0, 0)").unwrap().1;
    assert_eq!(SIUnit { s: 1, m: 0, kg: 0, a: 0, k: 0, mol: 0, cd: 0 }, unit);
    assert_eq!(1.0, scale);

    let (unit, scale) = parse_si_unit("SI(1000, 0, 1, 0, 0, 0, 0, 0)").unwrap().1;
    assert_eq!(SIUnit { s: 0, m: 1, kg: 0, a: 0, k: 0, mol: 0, cd: 0 }, unit);
    assert_eq!(1000.0, scale);

    let (unit, scale) = parse_si_unit("SI(1000)").unwrap().1;
    assert_eq!(SIUnit { s: 0, m: 0, kg: 0, a: 0, k: 0, mol: 0, cd: 0 }, unit);
    assert_eq!(1000.0, scale);
}

#[test]
fn test_try_parse_type_expr() {
    let expr = TypeExpr::<DemoType, ParamRef>::try_parse("Integer | String").unwrap();
    assert_eq!(expr, parse_type_expr::<DemoType, ParamRef>("Integer | String").unwrap().1);

    let expr: TypeExpr<DemoType> = "Boolean".parse().unwrap();
    assert_eq!(expr, TypeExpr::Type(DemoType::Boolean));

    let err = TypeExpr::<DemoType, ParamRef>::try_parse("Integer |").unwrap_err();
    assert!(!err.remaining.is_empty());
    assert!(err.offset > 0);
}

#[test]
fn test_try_parse_node_signature() {
    let sig = NodeSignature::<DemoType, ParamRef>::try_parse("() -> ()").unwrap();
    assert_eq!(sig, NodeSignature::default());

    let sig: NodeSignature<DemoType, ParamRef> = "(Integer) -> (String)".parse().unwrap();
    assert_eq!(
        sig.inputs,
        TypeExpr::PortTypes(Box::new(PortTypes::from_ports(vec![TypeExpr::Type(DemoType::Integer)])))
    );

    let err = NodeSignature::<DemoType, ParamRef>::try_parse("() -> () extra").unwrap_err();
    assert!(err.remaining.contains("extra"));
}

#[test]
fn test_parse_error_display() {
    let result = TypeExpr::<DemoType>::try_parse("??? invalid");
    assert!(result.is_err());
    let display = format!("{}", result.unwrap_err());
    assert!(display.contains("parse error"));
}

#[test]
fn test_parse_remaining_input_error() {
    assert!(TypeExpr::<DemoType>::try_parse("Integer GARBAGE").is_err());
}

#[test]
fn test_parse_node_signature_remaining_input_error() {
    assert!(NodeSignature::<DemoType>::try_parse("() -> () GARBAGE").is_err());
}

#[test]
fn test_parse_scope_from_str() {
    let scope: Scope<DemoType> = "<T, U extends T>".parse().unwrap();
    assert_eq!(scope.variables().len(), 2);
}

#[test]
fn test_parse_scope_remaining_error() {
    assert!(Scope::<DemoType>::try_parse("<T> GARBAGE").is_err());
}

#[test]
fn test_parse_type_hints() {
    let hints: nodety::TypeHints<DemoType, ParamRef> = "T = Integer, U = String".parse().unwrap();
    assert_eq!(hints.len(), 2);
}

#[test]
fn test_parse_varg_only() {
    let sig = sig_u("(...Integer) -> ()");
    assert_matches!(sig.inputs, TypeExpr::PortTypes(ref p) => {
        assert!(p.ports.is_empty());
        assert!(p.varg.is_some());
    });
}

#[test]
fn test_parse_empty_type_parameters() {
    let params: TypeParameters<DemoType, ParamRef> = "<>".parse().unwrap();
    assert!(params.is_empty());

    let params = parse_type_parameter_declarations::<DemoType, ParamRef>("<>").unwrap().1;
    assert!(params.is_empty());

    // Node signature with explicit empty type params
    let sig = sig("<> () -> ()");
    assert!(sig.parameters.is_empty());
}

#[test]
fn test_type_parameters_from_str_display_roundtrip() {
    let params: TypeParameters<DemoType, ParamRef> = "<T, U extends Integer>".parse().unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params.to_string(), "<T, U extends Integer>");

    let empty: TypeParameters<DemoType, ParamRef> = "<>".parse().unwrap();
    assert_eq!(empty.to_string(), "<>");
}
