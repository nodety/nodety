//! Properties of mutable constructors and `readonly`, on concrete arrays, records and unions of them.
use nodety::{
    TypeExpr,
    demo_type::DemoType,
    type_expr::{ConcreteTypeExpr, ConstructorParams},
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn mutable_values_are_readable(t in containers()) {
        prop_assert!(readonly(t.clone()).supertype_of_naive(&t).is_supertype(), "{t}");
    }

    #[test]
    fn readonly_is_idempotent(t in containers()) {
        prop_assert_eq!(readonly(readonly(t.clone())).normalize_naive(), readonly(t).normalize_naive());
    }

    #[test]
    fn readonly_keeps_subtyping(parent in containers(), child in containers()) {
        if parent.supertype_of_naive(&child).is_supertype() {
            prop_assert!(readonly(parent.clone()).supertype_of_naive(&readonly(child.clone())).is_supertype(),
                "{parent} :> {child}");
        }
    }

    #[test]
    fn mutable_parameters_are_invariant(a in containers(), b in containers()) {
        if mutable_array(a.clone()).supertype_of_naive(&mutable_array(b.clone())).is_supertype() {
            prop_assert!(a.supertype_of_naive(&b).is_supertype() && b.supertype_of_naive(&a).is_supertype(),
                "{a}, {b}");
        }
    }
}

fn containers() -> impl Strategy<Value = ConcreteTypeExpr<DemoType>> {
    let leaf = prop_oneof![
        Just(DemoType::Integer),
        Just(DemoType::String(None)),
        Just(DemoType::String(Some("a".into()))),
        Just(DemoType::Unit),
    ]
    .prop_map(TypeExpr::Type);
    leaf.prop_recursive(3, 24, 2, |element| {
        let inner = prop_oneof![
            Just(DemoType::Array),
            Just(DemoType::MutableArray),
            Just(DemoType::Record),
            Just(DemoType::MutableRecord),
        ];
        prop_oneof![
            (inner, element.clone(), proptest::option::of(element.clone())).prop_map(|(inner, a, b)| {
                let mut parameters = ConstructorParams::new();
                let first =
                    if matches!(inner, DemoType::Array | DemoType::MutableArray) { "elements_type" } else { "a" };
                parameters.insert(first, a);
                if let Some(b) = b.filter(|_| first == "a") {
                    parameters.insert("b", b);
                }
                TypeExpr::Constructor { inner, parameters }
            }),
            (element.clone(), element).prop_map(|(a, b)| a.union_with(b)),
        ]
    })
}

fn readonly(t: ConcreteTypeExpr<DemoType>) -> ConcreteTypeExpr<DemoType> {
    TypeExpr::Readonly(Box::new(t))
}

fn mutable_array(elements_type: ConcreteTypeExpr<DemoType>) -> ConcreteTypeExpr<DemoType> {
    TypeExpr::Constructor {
        inner: DemoType::MutableArray,
        parameters: [("elements_type".into(), elements_type)].into(),
    }
}
