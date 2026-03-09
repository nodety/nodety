use nodety::{
    NoOperator, Type, TypeExpr,
    notation::{format::FormattableType, parse::ParsableType},
    scope::ScopePointer,
    type_expr::{ScopePortal, TypeExprScope, Unscoped, node_signature::port_types::PortTypes},
};
use nom::{IResult, Parser, bytes::complete::tag, combinator::value};
use proptest::prelude::*;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    str::FromStr,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    #[test]
    fn test_parsing_roundtrip(expr in any::<TypeExpr<SimpleType>>()) {
        let expr = normalize_simple(expr);

        let formatted = format!("{}", expr);
        let parsed = TypeExpr::from_str(&formatted).unwrap();
        prop_assert_eq!(normalize_simple(expr), normalize_simple(parsed));
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SimpleType {
    Atom,
}

impl Type for SimpleType {
    type Operator = NoOperator;
}

impl FormattableType for SimpleType {
    fn format_type(
        &self,
        _parameters: Option<&BTreeMap<String, TypeExpr<Self>>>,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Self::Atom => write!(f, "Atom"),
        }
    }

    fn format_operator(_operator: &NoOperator, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *_operator {}
    }
}

impl ParsableType for SimpleType {
    fn parse<S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<Self, S>> {
        value(TypeExpr::Type(SimpleType::Atom), tag("Atom")).parse(input)
    }

    fn parse_operator(input: &str) -> IResult<&str, NoOperator> {
        Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag)))
    }
}

impl Arbitrary for SimpleType {
    type Parameters = ();
    type Strategy = BoxedStrategy<SimpleType>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        Just(SimpleType::Atom).boxed()
    }
}

fn normalize_simple(expr: TypeExpr<SimpleType>) -> TypeExpr<SimpleType> {
    let mut scoped: TypeExpr<SimpleType, ScopePortal<SimpleType>> = expr.into();
    scoped.traverse_mut(
        &ScopePointer::new_root(),
        &mut |expr, _scope, _| match expr {
            TypeExpr::Constructor { inner, .. } => {
                *expr = TypeExpr::Type(inner.clone());
            }
            TypeExpr::NodeSignature(sig) => {
                sig.tags = Some(HashSet::new());
                sig.required_tags = HashSet::new();
                sig.default_input_types.clear();
                for field in [&mut sig.inputs, &mut sig.outputs] {
                    if !matches!(field, TypeExpr::PortTypes(_) | TypeExpr::Any | TypeExpr::Never) {
                        *field = TypeExpr::PortTypes(Box::new(PortTypes::new()));
                    }
                }
            }
            _ => {}
        },
        false,
    );
    scoped.try_into_unscoped().expect("Unscoped can't have scope portals")
}
