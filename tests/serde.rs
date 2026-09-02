//! Test that the serde derives actually work.
use nodety::{NoOperator, Type, TypeExpr};
use serde::{Deserialize, Serialize};

fn assert_serde<T: Serialize + for<'de> Deserialize<'de>>(_val: &T) {}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum MyTypeWithoutOperator {
    Integer,
    String,
}

impl Type for MyTypeWithoutOperator {
    type Operator = NoOperator;
}

/// used to fail because NoOperator was missing the serde derives.
#[test]
fn test_serialize_without_operator() {
    let expr = TypeExpr::<MyTypeWithoutOperator>::Type(MyTypeWithoutOperator::Integer);
    assert_serde(&expr);
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum MyTypeWithOperator {
    Integer,
    String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub enum MyOperator {
    Multiply,
    Divide,
}

impl Type for MyTypeWithOperator {
    type Operator = MyOperator;
}

#[test]
fn test_serialize_with_operator() {
    let expr = TypeExpr::<MyTypeWithOperator>::Operation {
        a: Box::new(TypeExpr::<MyTypeWithOperator>::Type(MyTypeWithOperator::Integer)),
        operator: MyOperator::Multiply,
        b: Box::new(TypeExpr::<MyTypeWithOperator>::Type(MyTypeWithOperator::Integer)),
    };
    assert_serde(&expr);
}
