//! Parsing of type expressions in a typescript like syntax.
//!
//! **Note:** The parsers defined here are not optimized for performance.
//! When parsing many types, consider implementing caching of parsed types.
//!
//! **A note on scopes (`S`)** Since parsers never parse the [TypeExpr::ScopePortal] variant, they can parse any `S`.
//! For this reason all parsers are generic over S.
#[cfg(test)]
use crate::type_expr::ScopedTypeExpr;
use crate::{
    demo_type::{DemoOperator, DemoType, SIUnit},
    scope::{LocalParamID, Scope, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{
        ScopePortal, TypeExpr, TypeExprScope, Unscoped,
        conditional::Conditional,
        node_signature::{NodeSignature, port_types::PortTypes},
    },
};
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{alpha1, alphanumeric1, char, digit1, multispace0, multispace1, space0, space1},
    combinator::{map, opt, recognize, value},
    error::ParseError as NomParseError,
    multi::{many0, separated_list0, separated_list1},
    number::complete::double,
    sequence::{delimited, pair, separated_pair},
};
use nom::{multi::fold_many0, sequence::preceded};
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

/// Error returned when parsing a type expression or node signature fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable description of what went wrong.
    pub message: String,
    /// The unparsed input remaining when the error occurred.
    pub remaining: String,
    /// Byte offset into the original input where parsing failed.
    pub offset: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at offset {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    fn from_nom_err(original: &str, err: nom::Err<nom::error::Error<&str>>) -> Self {
        match &err {
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                let remaining = e.input.to_string();
                let offset = original.len().saturating_sub(e.input.len());
                Self { message: format!("{:?}", e.code), remaining, offset }
            }
            nom::Err::Incomplete(_) => Self {
                message: "incomplete input".to_string(),
                remaining: original.to_string(),
                offset: original.len(),
            },
        }
    }
}

/// Trait for types that can be parsed from text notation (e.g. `"<T>(T) -> (T)"`).
pub trait ParsableType
where
    Self: Type,
{
    fn parse<S: TypeExprScope + Clone>(input: &str) -> IResult<&str, TypeExpr<Self, S>>;
    fn parse_operator(input: &str) -> IResult<&str, Self::Operator>;
}

fn ws0<'a, O, E: NomParseError<&'a str>, F>(inner: F) -> impl Parser<&'a str, Output = O, Error = E>
where
    F: Parser<&'a str, Output = O, Error = E>,
{
    delimited(multispace0, inner, multispace0)
}

fn ws1<'a, O, E: NomParseError<&'a str>, F>(inner: F) -> impl Parser<&'a str, Output = O, Error = E>
where
    F: Parser<&'a str, Output = O, Error = E>,
{
    delimited(multispace1, inner, multispace1)
}

pub fn ident(input: &str) -> IResult<&str, &str> {
    alphanumeric1(input)
}

pub fn parse_type_parameter(input: &str) -> IResult<&str, (LocalParamID, bool)> {
    (
        opt(char('!')),
        alt((
            (char('#'), digit1).map(|(_, digits): (char, &str)| LocalParamID(digits.parse().unwrap())),
            ident.map(|ident| ident.into()),
        )),
    )
        .map(|(skip_infer, id)| (id, skip_infer.is_none()))
        .parse(input)
}

pub fn parse_type_expr<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    alt((
        parse_type_expr_conditional,
        parse_type_expr_union,
        parse_type_expr_intersection,
        parse_type_expr_index,
        parse_type_expr_operation,
        parse_atomic_type_expr,
    ))
    .parse(input)
}

pub fn parse_type_expr_union<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    let (input, first) = parse_atomic_type_expr(input)?;
    let (input, rest) = many0((ws0(char('|')), parse_atomic_type_expr)).parse(input)?;

    if rest.is_empty() {
        return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag)));
    }

    let result = rest.into_iter().fold(first, |acc, (_, expr)| TypeExpr::Union(Box::new(acc), Box::new(expr)));

    Ok((input, result))
}

fn parse_type_expr_index<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    (parse_atomic_type_expr, char('['), parse_type_expr, char(']'))
        .map(|(expr, _, index, _)| TypeExpr::Index { expr: Box::new(expr), index: Box::new(index) })
        .parse(input)
}

fn parse_type_expr_in_brackets<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    (ws0(char('(')), parse_type_expr, multispace0, char(')')).map(|(_, expr, _, _)| expr).parse(input)
}

fn parse_type_expr_intersection<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    let (input, first) = parse_atomic_type_expr(input)?;
    let (input, rest) = many0((ws0(char('&')), parse_atomic_type_expr)).parse(input)?;

    if rest.is_empty() {
        return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Tag)));
    }

    let result = rest.into_iter().fold(first, |acc, (_, expr)| TypeExpr::Intersection(Box::new(acc), Box::new(expr)));

    Ok((input, result))
}

fn parse_type_expr_operation<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    (parse_atomic_type_expr, ws0(T::parse_operator), parse_atomic_type_expr)
        .map(|(a, operator, b)| TypeExpr::Operation { a: Box::new(a), operator, b: Box::new(b) })
        .parse(input)
}

fn parse_type_expr_conditional<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    (
        parse_atomic_type_expr,
        ws1(tag("extends")),
        parse_atomic_type_expr,
        ws0(char('?')),
        parse_atomic_type_expr,
        ws0(char(':')),
        parse_atomic_type_expr,
    )
        .map(|(t_test, _extends, t_test_bound, _, t_then, _, t_else)| {
            TypeExpr::Conditional(Box::new(Conditional { t_test, t_test_bound, t_then, t_else, infer: HashSet::new() }))
        })
        .parse(input)
}

fn parse_atomic_type_expr<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    alt((
        parse_node_signature.map(|sig| TypeExpr::NodeSignature(Box::new(sig))),
        T::parse,
        value(TypeExpr::Any, tag("Any")),
        value(TypeExpr::Never, tag("Never")),
        parse_keyof,
        parse_type_parameter.map(|(id, infer)| TypeExpr::TypeParameter(id, infer)),
        parse_type_expr_in_brackets,
    ))
    .parse(input)
}

/// # Parses
/// (type expr, default types)
#[allow(clippy::type_complexity)]
fn parse_atomic_type_expr_with_ports<T: ParsableType, S: TypeExprScope>(
    input: &str,
) -> IResult<&str, (TypeExpr<T, S>, BTreeMap<usize, TypeExpr<T, S>>)> {
    alt((
        value((TypeExpr::Any, BTreeMap::new()), tag("Any")),
        value((TypeExpr::Never, BTreeMap::new()), tag("Never")),
        parse_port_types,
    ))
    .parse(input)
}

fn parse_keyof<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<T, S>> {
    (ws0(tag("keyof ")), parse_atomic_type_expr).map(|(_, expr)| TypeExpr::KeyOf(Box::new(expr))).parse(input)
}

fn parse_type_parameter_declaration<T: ParsableType, S: TypeExprScope>(
    input: &str,
) -> IResult<&str, (LocalParamID, TypeParameter<T, S>)> {
    (
        parse_type_parameter,
        opt((space1, tag("extends"), space1, parse_type_expr)),
        opt((space0, tag("="), space0, parse_type_expr)),
    )
        .map(|((param, _infer), bound, default)| {
            (
                param,
                TypeParameter {
                    bound: bound.map(|(_, _, _, bound)| bound),
                    default: default.map(|(_, _, _, default)| default),
                },
            )
        })
        .parse(input)
}

pub fn parse_type_parameter_declarations<T: ParsableType, S: TypeExprScope>(
    input: &str,
) -> IResult<&str, BTreeMap<LocalParamID, TypeParameter<T, S>>> {
    (char('<'), separated_list1(ws0(char(',')), parse_type_parameter_declaration), char('>'))
        .map(|(_, params, _)| params.into_iter().collect())
        .parse(input)
}

/// # Parses
/// (port types, default types)
#[allow(clippy::type_complexity)]
fn parse_port_types<T: ParsableType, S: TypeExprScope>(
    input: &str,
) -> IResult<&str, (TypeExpr<T, S>, BTreeMap<usize, TypeExpr<T, S>>)> {
    alt((
        (char('('), space0, char(')'))
            .map(|_| (TypeExpr::PortTypes(Box::new(PortTypes::<T, S> { ports: vec![], varg: None })), BTreeMap::new())),
        (
            ws0(char('(')),
            separated_list1(
                ws0(char(',')),
                (opt((parse_identifier, ws0(char(':')))), parse_type_expr, opt((ws0(char('=')), parse_type_expr))),
            ),
            opt((ws0(char(',')), ws0(tag("...")), parse_type_expr)),
            preceded(multispace0, char(')')),
        )
            .map(|(_, args, varg, _)| {
                let mut ports = Vec::new();
                let mut default_types: BTreeMap<usize, TypeExpr<T, S>> = BTreeMap::new();
                for (i, (_ident, port_type, default_type)) in args.into_iter().enumerate() {
                    ports.push(port_type);
                    if let Some((_, default_type)) = default_type {
                        default_types.insert(i, default_type);
                    }
                }
                (TypeExpr::PortTypes(Box::new(PortTypes { ports, varg: varg.map(|v| v.2) })), default_types)
            }),
        (char('('), ws0(tag("...")), parse_type_expr, space0, char(')')).map(|(_, _, varg, _, _)| {
            (TypeExpr::PortTypes(Box::new(PortTypes::<T, S> { ports: vec![], varg: Some(varg) })), BTreeMap::new())
        }),
    ))
    .parse(input)
}

fn parse_node_signature<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, NodeSignature<T, S>> {
    (
        opt(parse_type_parameter_declarations),
        space0,
        parse_atomic_type_expr_with_ports,
        ws0(tag("->")),
        parse_atomic_type_expr_with_ports,
    )
        .map(|(params, _, (inputs, default_input_types), _, (outputs, _discarded_default_outputs))| NodeSignature {
            parameters: params.unwrap_or(BTreeMap::new()),
            inputs,
            outputs,
            default_input_types,
            ..Default::default()
        })
        .parse(input)
}

impl ParsableType for DemoType {
    fn parse<S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<Self, S>> {
        alt((
            value(TypeExpr::Type(DemoType::Integer), tag("Integer")),
            value(TypeExpr::Type(DemoType::Float), tag("Float")),
            value(TypeExpr::Type(DemoType::String(None)), tag("String")),
            value(TypeExpr::Type(DemoType::Boolean), tag("Boolean")),
            value(TypeExpr::Type(DemoType::Countable), tag("Countable")),
            value(TypeExpr::Type(DemoType::Comparable), tag("Comparable")),
            value(TypeExpr::Type(DemoType::Sortable), tag("Sortable")),
            value(TypeExpr::Type(DemoType::Unit), tag("Unit")),
            value(TypeExpr::Type(DemoType::AnySI), tag("AnySI")),
            parse_quoted_string.map(|s| TypeExpr::Type(DemoType::String(Some(s)))),
            map(parse_si_unit, |(unit, scale)| TypeExpr::Type(DemoType::SI(unit, scale))),
            parse_array,
            parse_record.map(|parameters| TypeExpr::Constructor { inner: DemoType::Record, parameters }),
        ))
        .parse(input)
    }

    fn parse_operator(input: &str) -> IResult<&str, Self::Operator> {
        alt((value(DemoOperator::Multiplication, char('*')), value(DemoOperator::Division, char('/')))).parse(input)
    }
}

/// Parses SI type: `SI(scale, s, m, kg, a, k, mol, cd)` — scale first, then 7 exponents.
pub fn parse_si_unit(input: &str) -> IResult<&str, (SIUnit, f64)> {
    let (rest, (_, _, values, _)) =
        (tag("SI"), ws0(char('(')), separated_list0(ws0(char(',')), double), ws0(char(')'))).parse(input)?;
    if values.len() > 8 {
        return Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::LengthValue)));
    }

    let scale = values.first().copied().unwrap_or(1.0);
    let unit = SIUnit {
        s: values.get(1).copied().unwrap_or(0.0).round() as i16,
        m: values.get(2).copied().unwrap_or(0.0).round() as i16,
        kg: values.get(3).copied().unwrap_or(0.0).round() as i16,
        a: values.get(4).copied().unwrap_or(0.0).round() as i16,
        k: values.get(5).copied().unwrap_or(0.0).round() as i16,
        mol: values.get(6).copied().unwrap_or(0.0).round() as i16,
        cd: values.get(7).copied().unwrap_or(0.0).round() as i16,
    };
    Ok((rest, (unit, scale)))
}

fn parse_array<S: TypeExprScope>(input: &str) -> IResult<&str, TypeExpr<DemoType, S>> {
    (tag("Array"), opt((char('<'), parse_type_expr, char('>'))))
        .map(|(_, elements_type)| match elements_type {
            Some((_, elements_type, _)) => TypeExpr::Constructor {
                inner: DemoType::Array,
                parameters: BTreeMap::from([("elements_type".into(), elements_type)]),
            },
            None => TypeExpr::Type(DemoType::Array),
        })
        .parse(input)
}

fn parse_record_field_name(input: &str) -> IResult<&str, String> {
    alt((parse_quoted_string, parse_identifier.map(|s: &str| s.to_string()))).parse(input)
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(alt((alpha1, tag("_"))), many0(alt((alphanumeric1, tag("_"), tag("-")))))).parse(input)
}

pub fn parse_record<T: ParsableType, S: TypeExprScope>(input: &str) -> IResult<&str, BTreeMap<String, TypeExpr<T, S>>> {
    map(
        delimited(
            ws0(char('{')),
            separated_list0(ws0(char(',')), separated_pair(parse_record_field_name, ws0(char(':')), parse_type_expr)),
            ws0(char('}')),
        ),
        |parameters| parameters.into_iter().collect(),
    )
    .parse(input)
}

fn parse_escaped_char(input: &str) -> IResult<&str, char> {
    preceded(
        char('\\'),
        alt((
            value('\"', char('\"')),
            value('\\', char('\\')),
            value('\n', char('n')),
            value('\r', char('r')),
            value('\t', char('t')),
        )),
    )
    .parse(input)
}

pub fn parse_string_double(input: &str) -> IResult<&str, String> {
    fold_many0(
        alt((map(parse_escaped_char, |c| c.to_string()), map(is_not("\\\""), |s: &str| s.to_string()))),
        String::new,
        |mut acc, item| {
            acc.push_str(&item);
            acc
        },
    )
    .parse(input)
}

pub fn parse_string_single(input: &str) -> IResult<&str, String> {
    fold_many0(
        alt((map(parse_escaped_char, |c| c.to_string()), map(is_not("\\\'"), |s: &str| s.to_string()))),
        String::new,
        |mut acc, item| {
            acc.push_str(&item);
            acc
        },
    )
    .parse(input)
}

pub fn parse_quoted_string(input: &str) -> IResult<&str, String> {
    alt((
        delimited(char('\"'), parse_string_double, char('\"')),
        delimited(char('\''), parse_string_single, char('\'')),
    ))
    .parse(input)
}

/// Type hints are not part of the notation.
/// However this function can be useful to parse them separately.
///
/// Example:
/// ```
/// # use maplit::btreemap;
/// # use nodety::{notation::parse::parse_type_hints, type_expr::{TypeExpr, Unscoped}};
/// # use nodety::demo_type::DemoType;
/// let (_, hints) = parse_type_hints::<DemoType, Unscoped>("T = Integer, U = String").unwrap();
/// let expected = btreemap! {"T".into() => TypeExpr::Type(DemoType::Integer), "U".into() => TypeExpr::Type(DemoType::String(None))};
/// assert_eq!(hints, expected);
/// ```
pub fn parse_type_hints<T: ParsableType, S: TypeExprScope>(
    s: &str,
) -> IResult<&str, BTreeMap<LocalParamID, TypeExpr<T, S>>> {
    separated_list0(ws0(char(',')), (parse_type_parameter, ws0(char('=')), parse_type_expr))
        .map(|items| {
            items
                .into_iter()
                .map(|((param, _infer), _, hint)| (param, hint))
                .collect::<BTreeMap<LocalParamID, TypeExpr<T, S>>>()
        })
        .parse(s)
}

impl<T: ParsableType, S: TypeExprScope> NodeSignature<T, S> {
    #[deprecated(since = "0.1.1", note = "use the FromStr trait instead")]
    pub fn parse(input: &str) -> IResult<&str, Self> {
        parse_node_signature(input)
    }

    /// Parse the entire input as a node signature. Returns an error if parsing fails
    /// or if any input remains after parsing.
    pub fn try_parse(input: &str) -> Result<Self, ParseError> {
        match parse_node_signature(input) {
            Ok((rest, sig)) => {
                if rest.trim().is_empty() {
                    Ok(sig)
                } else {
                    Err(ParseError {
                        message: format!("unexpected remaining input: {:?}", rest),
                        remaining: rest.to_string(),
                        offset: input.len() - rest.len(),
                    })
                }
            }
            Err(e) => Err(ParseError::from_nom_err(input, e)),
        }
    }
}

impl<T: ParsableType, S: TypeExprScope> TypeExpr<T, S> {
    #[deprecated(since = "0.1.1", note = "use the FromStr trait instead")]
    pub fn parse(input: &str) -> IResult<&str, Self> {
        parse_type_expr(input)
    }

    /// Parse the entire input as a type expression. Returns an error if parsing fails
    /// or if any input remains after parsing.
    pub fn try_parse(input: &str) -> Result<Self, ParseError> {
        match parse_type_expr(input) {
            Ok((rest, expr)) => {
                if rest.trim().is_empty() {
                    Ok(expr)
                } else {
                    Err(ParseError {
                        message: format!("unexpected remaining input: {:?}", rest),
                        remaining: rest.to_string(),
                        offset: input.len() - rest.len(),
                    })
                }
            }
            Err(e) => Err(ParseError::from_nom_err(input, e)),
        }
    }
}

impl<T: ParsableType> Scope<T> {
    /// Parse the entire input as scope. Returns an error if parsing fails
    /// or if any input remains after parsing.
    pub fn try_parse(input: &str) -> Result<Self, ParseError> {
        match parse_type_parameter_declarations::<T, Unscoped>(input) {
            Ok((rest, params)) => {
                if rest.trim().is_empty() {
                    let mut scope = Scope::new_root();
                    for (param_id, param) in params {
                        scope.define(
                            param_id,
                            <TypeParameter<T, Unscoped> as Into<TypeParameter<T, ScopePortal<T>>>>::into(param),
                        );
                    }
                    Ok(scope)
                } else {
                    Err(ParseError {
                        message: format!("unexpected remaining input: {:?}", rest),
                        remaining: rest.to_string(),
                        offset: input.len() - rest.len(),
                    })
                }
            }
            Err(e) => Err(ParseError::from_nom_err(input, e)),
        }
    }
}

impl<T: ParsableType, S: TypeExprScope> FromStr for TypeExpr<T, S> {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TypeExpr::try_parse(s)
    }
}

impl<T: ParsableType, S: TypeExprScope> FromStr for NodeSignature<T, S> {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        NodeSignature::try_parse(s)
    }
}

impl<T: ParsableType> FromStr for Scope<T> {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Scope::try_parse(s)
    }
}

// impl<T: ParsableType> FromStr for ScopePointer<T> {
//     type Err = ParseError;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         Scope::try_parse(s)
//     }
// }

/// Shorthand for tests.
#[cfg(test)]
#[track_caller]
pub(crate) fn sig(input: &str) -> NodeSignature<DemoType, ScopePortal<DemoType>> {
    NodeSignature::from_str(input).expect(&format!("Failed to parse {input}"))
}

/// Shorthand for tests.
#[cfg(test)]
#[track_caller]
#[allow(dead_code)]
pub(crate) fn sig_u(input: &str) -> NodeSignature<DemoType, Unscoped> {
    NodeSignature::from_str(input).expect(&format!("Failed to parse {input}"))
}

/// Shorthand for tests.
#[cfg(test)]
#[track_caller]
pub(crate) fn expr(input: &str) -> ScopedTypeExpr<DemoType> {
    TypeExpr::<DemoType, ScopePortal<DemoType>>::from_str(input).expect(&format!("Failed to parse {input}"))
}

/// Shorthand for tests.
#[cfg(test)]
#[track_caller]
pub(crate) fn scope(input: &str) -> Scope<DemoType> {
    if input == "<>" {
        return Scope::new_root();
    }
    let (rest, params) = parse_type_parameter_declarations::<DemoType, Unscoped>(input).unwrap();
    if rest.len() > 0 {
        panic!("Failed to parse scope {input}! Not everything got parsed. missing {rest}");
    }
    let mut scope = Scope::new_root();
    for (param_id, param) in params {
        scope.define(param_id, param.into());
    }
    scope
}

#[cfg(test)]
pub mod test {
    use crate::notation::parse::parse_identifier;

    #[test]
    fn test_parse_identifier() {
        assert_eq!((" ef", "_ab-cd-1"), parse_identifier("_ab-cd-1 ef").unwrap());
        assert!(parse_identifier("").is_err());
    }
}
