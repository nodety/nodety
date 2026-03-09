use crate::{
    demo_type::{DemoOperator, DemoType},
    scope::{LocalParamID, type_parameter::TypeParameter},
    r#type::Type,
    type_expr::{TypeExpr, Unscoped, node_signature::NodeSignature},
};
use core::fmt;
use std::collections::BTreeMap;

/// Trait for types that can be formatted to text notation (e.g. `"<T>(T) -> (T)"`).
///
/// Implement this for your types to enable formatting of `TypeExpr<YourType>`.
pub trait FormattableType: Type {
    fn format_type(
        &self,
        parameters: Option<&BTreeMap<String, TypeExpr<Self>>>,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result;

    fn format_operator(operator: &Self::Operator, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

/// Formats a string for use in type notation (quotes and escapes if needed).
pub fn format_string(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()) {
        s.to_string()
    } else {
        format!("\"{}\"", escape_string_content(s))
    }
}

fn escape_string_content(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            '\r' => vec!['\\', 'r'],
            '\t' => vec!['\\', 't'],
            c => vec![c],
        })
        .collect()
}

/// Wrapper for displaying type parameters in notation (e.g. `<T, U extends T>`).
pub struct TypeParamsDisplay<'a, T: FormattableType> {
    pub params: &'a BTreeMap<LocalParamID, TypeParameter<T>>,
}

/// Formats type parameters to notation (e.g. `<T extends Comparable, U = Any>`).
pub fn format_type_params<T: FormattableType>(
    params: &BTreeMap<LocalParamID, TypeParameter<T>>,
    f: &mut std::fmt::Formatter<'_>,
) -> fmt::Result {
    if params.is_empty() {
        return Ok(());
    }
    write!(f, "<")?;
    let mut first = true;
    for (ident, param) in params {
        if !first {
            write!(f, ", ")?;
        }
        format_type_param(*ident, f)?;
        if let Some(bound) = &param.bound {
            write!(f, " extends ")?;
            bound.format_type(f, false)?;
        }
        if let Some(default) = &param.default {
            write!(f, " = ")?;
            default.format_type(f, false)?;
        }
        first = false;
    }
    write!(f, ">")?;
    Ok(())
}

fn format_type_param(param_id: LocalParamID, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
    if (param_id.0 as u8).is_ascii_lowercase() || (param_id.0 as u8).is_ascii_uppercase() {
        let c = param_id.0 as u8 as char;
        write!(f, "{c}")?;
    } else {
        write!(f, "#{}", param_id.0)?;
    }
    Ok(())
}

impl FormattableType for DemoType {
    fn format_type(
        &self,
        parameters: Option<&BTreeMap<String, TypeExpr<Self>>>,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Self::Array => {
                write!(f, "Array")?;
                if let Some(elements_type) = parameters.as_ref().and_then(|params| params.get("elements_type")) {
                    write!(f, "<")?;
                    elements_type.format_type(f, false)?;
                    write!(f, ">")?;
                }
                Ok(())
            }
            Self::Boolean => Ok(write!(f, "Boolean")?),
            Self::Comparable => Ok(write!(f, "Comparable")?),
            Self::Countable => Ok(write!(f, "Countable")?),
            Self::Float => Ok(write!(f, "Float")?),
            Self::Integer => Ok(write!(f, "Integer")?),
            Self::Record => {
                f.write_str("{")?;
                if let Some(parameters) = parameters {
                    let mut first = true;
                    for (ident, t) in parameters {
                        if !first {
                            write!(f, ", ")?;
                        }
                        let ident = format_string(ident);
                        write!(f, "{ident}: ")?;
                        t.format_type(f, false)?;
                        first = false;
                    }
                }
                f.write_str("}")?;
                Ok(())
            }
            Self::Sortable => Ok(write!(f, "Sortable")?),
            Self::String(Some(literal)) => Ok(write!(f, "\"{literal}\"")?),
            Self::String(None) => Ok(write!(f, "String")?),
            Self::Unit => Ok(write!(f, "Unit")?),
            Self::SI(unit, scale) => {
                let values = [unit.s, unit.m, unit.kg, unit.a, unit.k, unit.mol, unit.cd];
                let last_non_zero = values.iter().rposition(|&v| v != 0).map_or(0, |i| i);
                write!(f, "SI({scale}")?;
                for &v in &values[..=last_non_zero] {
                    write!(f, ", {v}")?;
                }
                Ok(write!(f, ")")?)
            }
            Self::AnySI => Ok(write!(f, "AnySI")?),
        }
    }

    fn format_operator(operator: &<Self as Type>::Operator, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match operator {
            DemoOperator::Multiplication => Ok(write!(f, "*")?),
            DemoOperator::Division => Ok(write!(f, "/")?),
        }
    }
}

impl<T: FormattableType> NodeSignature<T> {
    pub fn format_type_notation(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        format_type_params(&self.parameters, f)?;
        self.inputs.format_type(f, true)?;
        write!(f, " -> ")?;
        self.outputs.format_type(f, true)
    }
}

impl<T: FormattableType> TypeExpr<T, Unscoped> {
    pub fn format_type(&self, f: &mut std::fmt::Formatter<'_>, atomic: bool) -> fmt::Result {
        match self {
            // scope is !
            Self::ScopePortal { scope: never, .. } => match *never {},

            Self::Any => Ok(write!(f, "Any")?),
            Self::Conditional(conditional) => {
                if atomic {
                    write!(f, "(")?;
                }
                conditional.t_test.format_type(f, true)?;
                write!(f, " extends ")?;
                conditional.t_test_bound.format_type(f, true)?;
                write!(f, " ? ")?;
                conditional.t_then.format_type(f, true)?;
                write!(f, " : ")?;
                conditional.t_else.format_type(f, true)?;
                if atomic {
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::Constructor { inner, parameters } => inner.format_type(Some(parameters), f),
            Self::Index { expr, index } => {
                if atomic {
                    write!(f, "(")?;
                }
                expr.format_type(f, true)?;
                write!(f, "[")?;
                index.format_type(f, false)?;
                write!(f, "]")?;
                if atomic {
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::Type(inst) => inst.format_type(None, f),
            Self::Intersection(a, b) => {
                if atomic {
                    write!(f, "(")?;
                    a.format_type(f, true)?;
                    write!(f, " & ")?;
                    b.format_type(f, true)?;
                    Ok(write!(f, ")")?)
                } else {
                    a.format_type(f, true)?;
                    write!(f, " & ")?;
                    b.format_type(f, true)
                }
            }
            Self::KeyOf(expr) => {
                if atomic {
                    write!(f, "(")?;
                }
                write!(f, "keyof ")?;
                expr.format_type(f, true)?;
                if atomic {
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::Operation { a, b, operator } => {
                if atomic {
                    write!(f, "(")?;
                }
                a.format_type(f, true)?;
                write!(f, " ")?;
                T::format_operator(operator, f)?;
                write!(f, " ")?;
                b.format_type(f, true)?;
                if atomic {
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::Never => Ok(write!(f, "Never")?),
            Self::NodeSignature(sig) => sig.format_type_notation(f),
            Self::PortTypes(ports) => {
                write!(f, "(")?;
                let mut first = true;
                for port in &ports.ports {
                    if !first {
                        write!(f, ", ")?;
                    }
                    port.format_type(f, false)?;
                    first = false;
                }
                if let Some(varg) = &ports.varg {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "...")?;
                    varg.format_type(f, false)?;
                }
                Ok(write!(f, ")")?)
            }
            Self::TypeParameter(param, infer) => {
                if !*infer {
                    write!(f, "!")?;
                }
                format_type_param(*param, f)?;
                Ok(())
            }
            Self::Union(a, b) => {
                if atomic {
                    write!(f, "(")?;
                    a.format_type(f, true)?;
                    write!(f, " | ")?;
                    b.format_type(f, true)?;
                    Ok(write!(f, ")")?)
                } else {
                    a.format_type(f, true)?;
                    write!(f, " | ")?;
                    b.format_type(f, true)
                }
            }
        }
    }
}

impl<T: FormattableType> std::fmt::Display for TypeExpr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.format_type(f, false)
    }
}

impl<T: FormattableType> std::fmt::Display for NodeSignature<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.format_type_notation(f)
    }
}

impl<T: FormattableType> std::fmt::Display for TypeParamsDisplay<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        format_type_params(self.params, f)
    }
}
