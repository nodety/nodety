use crate::{r#type::Type, type_expr::ParameterizedTypeExpr};

/// Types that know their [`TypeExpr`] representation.
///
/// Can be implemented for value types that know their own type.
pub trait ToTypeExpr {
    type Type: Type;
    /// Returns a type expression that is a supertype of self.
    /// Might return Any if self doesnt know about its type.
    fn to_type_expr(&self) -> ParameterizedTypeExpr<Self::Type>;
}
