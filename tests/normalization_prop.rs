use nodety::{TypeExpr, demo_type::DemoType, scope::ScopePointer, type_expr::ScopedTypeRef};
use proptest::prelude::*;

proptest! {
    #[test]
    #[ignore]
    fn test_normalization_is_equal_to_before(expr in any::<TypeExpr<DemoType>>()) {
        let scoped: TypeExpr<DemoType, ScopedTypeRef<DemoType>> = expr.into();
        let scope = ScopePointer::new_root();
        let normalized = scoped.normalize(&scope);

        prop_assert!(scoped.supertype_of(&normalized, &scope, &scope).is_supertype());
        prop_assert!(normalized.supertype_of(&scoped, &scope, &scope).is_supertype());
    }
}
