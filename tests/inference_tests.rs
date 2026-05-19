use crate::common::{expr, graph, sig, sig_u};
use DemoType::*;
use TypeExpr::*;
use maplit::btreemap;
use nodety::{
    demo_type::DemoType,
    inference::{InferenceConfig, InferenceStep},
    scope::LocalParamID,
    type_expr::TypeExpr,
};
use petgraph::graph::NodeIndex;

mod common;

///                  <T>
///  |    int| ----- |T    |
#[test]
fn test_infer_forwards() {
    let engine = graph(vec![sig_u("() -> (Integer)"), sig_u("<T>(T) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());
    let inferred_t = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();
    assert_eq!(inferred_t.0, Type(Integer));
}

///       <T>
///  |     T| ----- |int    |
#[test]
fn test_infer_backwards() {
    let engine = graph(vec![sig_u("<T>() -> (T)"), sig_u("(Integer) -> ()")], vec![(0, 1, 0, 0)]);

    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t = scopes.get(&NodeIndex::from(0)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();
    assert_eq!(inferred_t.0, Type(Integer));
}

///       <T>        <T>
///  |     T| ----- |T    T| ----- | String     |
#[test]
fn test_infer_multiple_backwards() {
    let engine = graph(
        vec![sig_u("<T>() -> (T)"), sig_u("<T>(T) -> (T)"), sig_u("(String) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    let inferred_t = scopes.get(&NodeIndex::from(0)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();
    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(String(None)));
}

///                   <T>            <T>
///  |  String| ----- |T    T| ----- | T   |
#[test]
fn test_infer_multiple_forwards() {
    let engine = graph(
        vec![sig_u("() -> (String)"), sig_u("<T>(T) -> (T)"), sig_u("<T>(T) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    let inferred_t = scopes.get(&NodeIndex::from(2)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();

    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(String(None)));
}

///   <#0>       <#1>
///  | #0| ----- |#1 |
///
/// Expected result:
/// U gets inferred to T and T does not get inferred at all because inferring it from U would create a cycle.
#[test]
fn test_infer_identity() {
    let engine = graph(vec![sig_u("<#0>() -> (#0)"), sig_u("<#1>(#1) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_0 = scopes.get(&NodeIndex::from(0)).unwrap().lookup_inferred(&LocalParamID(0));
    // dbg!(&inferred_0);
    let (inferred_1, ..) = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID(1)).unwrap();

    assert!(inferred_0.is_none());
    assert_eq!(inferred_1, TypeParameter(LocalParamID(0), true));
}

///                              <T, U>
///  |        Array<int>| ----- | Array<T>    |
///  | (int) -> (String)| ----- | (T) -> (U) |
///
/// Expected result:
/// T should infer to int
/// U should infer to String
#[test]
fn test_infer_map() {
    let engine = graph(
        vec![sig_u("() -> (Array<Integer>, (Integer) -> (String))"), sig_u("<T, U>(Array<T>, (T) -> (U)) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();
    let inferred_u = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("U")).unwrap();

    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(Integer));
    assert_eq!(inferred_u.0.normalize(&inferred_u.1), TypeExpr::Type(String(None)));
}

///                                  <T>
///  |   {a: int, b: string}| ----- | {a: T}    |
///
/// Expected result:
/// T should infer to int
#[test]
fn test_infer_record() {
    let engine = graph(vec![sig_u("() -> ({a: Integer, b: String})"), sig_u("<T>({a: T}) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());
    let inferred_t = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();

    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(Integer));
}

///  <T>                     <U>
///  | {a: int, b: T}| ----- |{a: U}    |
///
/// Expected result:
/// T doesn't get inferred
/// U gets inferred to int
#[test]
fn test_infer_record_both_sides_generic() {
    let engine = graph(vec![sig_u("<T>() -> ({a: Integer, b: T})"), sig_u("<U>({a: U}) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());
    let inferred_t = scopes.get(&NodeIndex::from(0)).unwrap().lookup_inferred(&LocalParamID::from("T"));
    let inferred_u = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("U")).unwrap();

    assert!(inferred_t.is_none());

    assert_eq!(inferred_u.0.normalize(&inferred_u.1), TypeExpr::Type(Integer));
}

///                    <T, U, M>
///  |  string| ----- | T (default type: Int)  |
///  |        |       | U (default type: Int)  |
///  |        |       | M (default type: U)    |
///
/// Expected result:
/// T should infer to String
/// U should infer to Int
/// M should infer to Int
#[test]
fn test_infer_default_types() {
    let mut sig_2 = sig_u("<T, U, M>(T , U, M) -> ()");
    sig_2.default_input_types = btreemap! {
        0 => TypeExpr::Type(DemoType::Integer),
        1 => TypeExpr::Type(DemoType::Integer),
        2 => TypeExpr::TypeParameter(LocalParamID::from("U"), true)
    };

    let engine = graph(vec![sig_u("() -> (String)"), sig_2], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();
    let inferred_u = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("U")).unwrap();
    let inferred_m = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("M")).unwrap();

    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(String(None)));
    assert_eq!(inferred_u.0.normalize(&inferred_u.1), TypeExpr::Type(Integer));
    assert_eq!(inferred_m.0.normalize(&inferred_m.1), TypeExpr::Type(Integer));
}

///               <T>
///  |  {}| ----- | {a: T} |
///  | Int| ----- | T      |
///
/// Expected result:
/// T gets inferred for Int
#[test]
fn test_infer_invalid_record() {
    let engine =
        graph(vec![sig_u("() -> ({}, Integer)"), sig_u("<T>({a: T}, T) -> ()")], vec![(0, 1, 0, 0), (0, 1, 1, 1)]);

    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();

    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(Integer));
}

///                      <T>
///  |        Int| ----- |T  |
///  | Comparable| ----- |T  |
///
/// Expected result:
/// T gets inferred to Comparable because that is the only common supertype.
#[test]
fn test_infer_best_common_supertype() {
    let engine =
        graph(vec![sig_u("() -> (Integer, Comparable)"), sig_u("<T>(T, T) -> ()")], vec![(0, 1, 0, 0), (0, 1, 1, 1)]);
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();

    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(Comparable));

    // Verify that the other way around works as well.
    let engine =
        graph(vec![sig_u("() -> (Comparable, Integer)"), sig_u("<T>(T, T) -> ()")], vec![(0, 1, 0, 0), (0, 1, 1, 1)]);
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t = scopes.get(&NodeIndex::from(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap();

    assert_eq!(inferred_t.0.normalize(&inferred_t.1), TypeExpr::Type(Comparable));
}

///  |  ((Integer, Integer) -> ())  | ----- | <T>((Integer, ...T) -> ()) -> ()  |
/// T should infer to Integer when matching the variadic against the second Integer
#[test]
fn test_infer_variadic_from_function() {
    let engine = graph(
        vec![sig_u("() -> ((Integer, Integer) -> ())"), sig_u("<T>((Integer, ...T) -> ()) -> ()")],
        vec![(0, 1, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t =
        TypeExpr::TypeParameter(LocalParamID::from("T"), true).normalize(scopes.get(&NodeIndex::from(1)).unwrap());

    assert_eq!(inferred_t, Type(Integer));
}

#[test]
fn test_infer_closure() {
    let engine = graph(
        vec![sig_u("<T extends Never -> Any>() -> (T)"), sig_u("((Integer, Integer) -> (Integer)) -> ()")],
        vec![(0, 1, 0, 0)],
    );
    // dbg!(sig("((Integer, Integer) -> (Integer) | Integer) -> ()"));
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t = TypeExpr::TypeParameter("T".into(), true).normalize(scopes.get(&NodeIndex::from(0)).unwrap());

    assert_eq!(TypeExpr::NodeSignature(Box::new(sig("(Integer, Integer) -> (Integer)"))), inferred_t);
}
///                                           <T, U>
///            Array<{prev: String}> | ----- | Array<T>    Array<U> |
///                                          |                      |
/// <T>(T) -> (T & {added: Integer}) | ----- | (T) -> (U)           |
#[test]
fn test_infer_generic_map() {
    let engine = graph(
        vec![
            sig_u("() -> (Array<{prev: String}>)"),
            sig_u("() -> (<T>(T) -> (T & {added: Integer}))"),
            sig_u("<T, U>(Array<T>, (T) -> (U)) -> (Array<U>)"),
        ],
        vec![(0, 2, 0, 0), (1, 2, 0, 1)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let scope2 = scopes.get(&NodeIndex::from(2)).unwrap();

    assert_eq!(expr("Array<{prev: String, added: Integer}>"), expr("Array<U>").normalize(scope2))
}

///  |  string| ----- | int |
#[test]
fn test_infer_invalid_types() {
    let engine = graph(vec![sig_u("() -> (String)"), sig_u("(Integer) -> ()")], vec![(0, 1, 0, 0)]);
    engine.infer(&InferenceConfig::default());
}

///                           <T>                   <U>
/// |   {a: Integer}| ------ |T     T['a']| ------- |U          |
#[test]
fn test_infer_from_keyof() {
    let engine = graph(
        vec![sig_u("() -> ({a: Integer})"), sig_u("<T>(T) -> (T['a'])"), sig_u("<U>(U) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_u =
        TypeExpr::TypeParameter(LocalParamID::from("U"), true).normalize(scopes.get(&NodeIndex::from(2)).unwrap());

    assert_eq!(expr("Integer"), inferred_u);
}

///                                  <T>                   <U>
/// |   {a: Array<Integer>}| ------ |T     T['a']| ------- |Array<U>          |
#[test]
fn test_infer_from_nested_keyof() {
    let engine = graph(
        vec![sig_u("() -> ({a: Array<Integer>})"), sig_u("<#0>(#0) -> (#0['a'])"), sig_u("<#1>(Array<#1>) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_1 = TypeExpr::TypeParameter(LocalParamID(1), true).normalize(scopes.get(&NodeIndex::from(2)).unwrap());

    assert_eq!(expr("Integer"), inferred_1);
}

/// This test used to fail because here:      
///  <T>                 <C>
/// |   T['abc] | ----- | C  |
///
/// C Used to get inferred from (bound of T)['abc'] before T was inferred.
#[test]
fn test_infer_from_index() {
    let engine = graph(
        // Reversed order because otherwise the test might falsely pass because #1 gets inferred first anyway
        vec![
            sig_u("<#1, #2 extends keyof #1>(Array<#1>) -> ()"),
            sig_u("<#0 extends {a: Array<Any>}>(#0 = {a: Array<Integer>}) -> (#0['a'])"),
        ],
        vec![(1, 0, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_1 = TypeExpr::TypeParameter(LocalParamID(1), true).normalize(scopes.get(&NodeIndex::from(0)).unwrap());

    assert_eq!(expr("Integer"), inferred_1);
}

/// Outdated
///  <T>                 <#0, #1>
///                      | Boolean       |       <T>
/// | T = 'a'  T | ----- | #0    #0 | #1 | ----- | T |
///                      |               |
/// | T = 'b'  T | ----- | #1            |
///
/// T should infer to 'a' | 'b'
#[test]
fn test_infer_from_ternary() {
    let engine = graph(
        // Reversed order because otherwise the test might falsely pass because #1 gets inferred first anyway
        vec![
            /* 0 */ sig_u("<T>(T = 'false-starts') -> (T)"),
            /* 1 */ sig_u("<T>(T = 'DQT') -> (T)"),
            /* 2 */ sig_u("<#1, #2>(Boolean, #1, #2) -> (#1 | #2)"),
            /* 3 */
            sig_u(
                "() -> ({ result_table: Array<{ time: Integer | Unit, DNS: Boolean, DNF: Boolean, DQT: Boolean, false-starts: Integer, DQS: Boolean }> })",
            ),
            /* 4 */ sig_u("<T>(T) -> (T['result_table'])"),
            /* 5 */ sig_u("<C, K extends keyof C>(Array<C>, K) -> ()"),
        ],
        vec![(0, 2, 0, 1), (1, 2, 0, 2), (3, 4, 0, 0), (4, 5, 0, 0), (2, 5, 0, 1)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t =
        TypeExpr::TypeParameter(LocalParamID::from("K"), true).normalize(scopes.get(&NodeIndex::from(5)).unwrap());

    assert_eq!(expr("'false-starts' | 'DQT'"), inferred_t);
}

#[test]
fn test_si_division() {
    let engine = graph(
        vec![
            // m / s
            sig_u("<A extends AnySI, B extends AnySI>(a: A = SI(1,0,1), b: B = SI(1,1)) -> (A / B)"),
            sig_u("<T>(T) -> ()"),
        ],
        vec![(0, 1, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_t =
        TypeExpr::TypeParameter(LocalParamID::from("T"), true).normalize(scopes.get(&NodeIndex::from(1)).unwrap());

    assert_eq!(expr("SI(1,-1,1)"), inferred_t);
}

/// # Example:                   <U>
/// |    <T>(T) -> (T) | ------- | (U) -> (Integer)    |
///
/// Expect T to get inferred from Integer on the fly during
/// candidate collection so that later U gets inferred to Integer.
#[test]
fn test_infer_during_candidate_collection() {
    let engine = graph(vec![sig_u("() -> (<T>(T) -> (T))"), sig_u("<U>((U) -> (Integer)) -> ()")], vec![(0, 1, 0, 0)]);
    let scopes = engine.infer(&InferenceConfig::default());

    let inferred_u =
        TypeExpr::TypeParameter(LocalParamID::from("U"), true).normalize(scopes.get(&NodeIndex::from(1)).unwrap());

    assert_eq!(expr("Integer"), inferred_u);

    // Test the same scenario again but this time with
    // infer_candidates = false

    let scopes = engine.infer(&InferenceConfig {
        steps: InferenceStep::default_steps()
            .into_iter()
            .map(|mut step| {
                step.infer_candidates = false;
                step
            })
            .collect(),
        ..Default::default()
    });

    let inferred_u =
        TypeExpr::TypeParameter(LocalParamID::from("U"), true).normalize(scopes.get(&NodeIndex::from(1)).unwrap());

    assert_eq!(expr("T"), inferred_u);
}

///                                         <T>
///  |  ((Integer, Integer) -> ())  | ----- | ((Integer, ...T) -> ()) -> ()  |
#[test]
fn test_infer_from_generic_varg() {
    let engine = graph(
        vec![sig_u("() -> ((Integer, Integer) -> ())"), sig_u("<T>((Integer, ...T) -> ()) -> ()")],
        vec![(0, 1, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());
    let inferred_t = scopes.get(&NodeIndex::new(1)).unwrap().lookup_inferred(&LocalParamID::from("T")).unwrap().0;
    assert_eq!(expr("Integer"), inferred_t);
}

/// Don't delete this test! It tests, that type params can only get inferred when their bounds are inferred.
/// More info in `Candidates::pick_best`.
///
///                  <T, U extends T>
///  |Integer| ----- |T            |
///  | String| ----- |U            |
#[test]
fn test_validate_invalid_bounds_dont_infer_generic() {
    let engine = graph(
        vec![sig_u("() -> (Integer, String)"), sig_u("<T, U extends T>(T, U) -> ()")],
        vec![(0, 1, 0, 0), (0, 1, 1, 1)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let scope = scopes.get(&NodeIndex::from(1)).unwrap();

    let (inferred_t, _) = scope.lookup_inferred(&LocalParamID::from("T")).unwrap();
    assert_eq!(expr("Integer"), inferred_t);

    let inferred_u = scope.lookup_inferred(&LocalParamID::from("U"));
    assert!(inferred_u.is_none());
}

///                         Element at index (optional)         
///                          <T>                                  <U>
///  |Array<Integer>| ----- |Array<T>             T | Unit| ---- | U      |
#[test]
fn test_infer_most_generic_type_from_element_at_index() {
    let engine = graph(
        vec![sig_u("() -> (Array<Integer>)"), sig_u("<T>(Array<T>) -> (T | Unit)"), sig_u("<U>(U) -> ()")],
        vec![(0, 1, 0, 0), (1, 2, 0, 0)],
    );
    let scopes = engine.infer(&InferenceConfig::default());

    let scope2 = scopes.get(&NodeIndex::from(2)).unwrap();

    let (inferred_u, inferred_u_scope) = scope2.lookup_inferred(&"U".into()).unwrap();
    assert_eq!(expr("Integer | Unit"), inferred_u.normalize(&inferred_u_scope));
}

// #[test]
// fn test_infer_outer_signature_identity() {
//     let mut nodety = Nodety::<DemoType>::new();
//     let root_id = nodety.add_node(sig_u("<I, O>() -> ()")).unwrap();

//     let input_node = nodety.add_node(Node::new_child(sig_u("() -> (I)"), root_id)).unwrap();
//     let output_node = nodety.add_node(Node::new_child(sig_u("(O) -> ()"), root_id)).unwrap();

//     nodety.add_edge(input_node, output_node, Edge { source_port: 0, target_port: 0 }).unwrap();

//     let scopes = nodety.infer(&InferenceConfig::default());

//     // let outer_sig = NodeSigna
// }

// pub fn build_outer_signature
