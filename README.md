[![Crates.io](https://img.shields.io/crates/v/nodety.svg)](https://crates.io/crates/nodety)
[![Documentation](https://docs.rs/nodety/badge.svg)](https://docs.rs/nodety/)
[![dependency status](https://deps.rs/repo/github/nodety/nodety/status.svg)](https://deps.rs/repo/github/timolehnertz/nodety)
[![Codecov](https://codecov.io/github/nodety/nodety/coverage.svg?branch=master)](https://codecov.io/gh/TimoLehnertz/nodety)

# Nodety

Generics, type inference, and validation for visual node editors.

Nodety gives your node graph a type system comparable to TypeScript.
Define node signatures like `<T, U>(Array<T>, (T) -> (U)) -> (Array<U>)`, wire them up, and let nodety infer `T = Integer`, `U = String` automatically.

## Features

- **Generics & inference** — `<T>(T) -> (T)`
- **Conditional types** — `T extends String ? String : Never`
- **Unions & intersections** — `"a" | "b"`, `{ a: Int } & { b: String }`
- **Keyof & index access** — `keyof { a: Int }`, `{ a: Int }["a"]`
- **Variadic ports (inputs & outputs)** — `(...Integer) -> (Array<Integer>)`
- **Rank-N polymorphism** — `(<T>(T) -> (T)) -> ()`
- **User-defined type operations** — `<A, B>(A, B) -> (A * B)` (e.g. SI units)
- **Propagating tags** — enforce constraints like const-correctness across the graph
- **Detailed diagnostics** — validation errors pinpoint the exact edge/port with subtyping traces

## Quick start

```toml
[dependencies]
nodety = "0.1"
```

```rust
// Mapper Example:
// Generic mapper node that infers the source type from an input
// array and the output from a mapper node signature.
//
// here nodety infers that
// T = Integer
// U = String
//
// |- Array source -----------|               |- <T, U>Map ----------------|
// |           Array<Integer> | ------------> | Array<T>          Array<U> |
// |--------------------------|               |                            |
//                                    /-----> | (T) -> (U)                 |
// |- Mapper -----------------|       |       |----------------------------|
// |    (Integer) -> (String) | ------/
// |--------------------------|

let mut nodety = Nodety::<DemoType>::new();

let source = "() -> (Array<Integer>)".parse::<NodeSignature<_>>().unwrap();
let mapper = "() -> ((Integer) -> (String))".parse::<NodeSignature<_>>().unwrap();
let map    = "<T, U>(Array<T>, (T) -> (U)) -> (Array<U>)".parse::<NodeSignature<_>>().unwrap();

let src_id = nodety.add_node(source).unwrap();
let map_id_fn = nodety.add_node(mapper).unwrap();
let map_id = nodety.add_node(map).unwrap();

nodety.add_edge(array_source_node_id, map_node_id, Edge { source_port: 0, target_port: 0 }).unwrap();
nodety.add_edge(mapper_node_id, map_node_id, Edge { source_port: 0, target_port: 1 }).unwrap();

let inference = nodety.infer(&InferenceConfig::default());
let errors = nodety.validate(&inference);
assert!(errors.is_empty());

// T = Integer, U = String
```

## Cargo features

| Feature       | Default | Description                                        |
| ------------- | ------- | -------------------------------------------------- |
| `parser`      | yes     | Parse signatures and type expressions from strings |
| `serde`       | no      | Serialize / deserialize types with serde           |
| `json-schema` | no      | JSON Schema generation via schemars                |
| `wasm`        | no      | wasm-bindgen support (enables `serde`)             |
| `tsify`       | no      | TypeScript type generation (enables `wasm`)        |
| `proptest`    | no      | `Arbitrary` impls for property testing             |

## Minimum Supported Rust Version (MSRV)

The MSRV is **1.85.0** (Rust edition 2024).

## Documentation

See the [crate docs](https://docs.rs/nodety) for the full guide — types, scopes, notation, and more.

## State of this crate

This crate is still in its early stages. The core architecture and features have been worked out. But the API might still change in the near future on the path to finding the most ergonomic abstractions.

## Contributing

Found a bug? Please [open an issue](https://github.com/TimoLehnertz/nodety/issues) with a minimal reproduction. Including a failing test case is especially helpful and will speed up fixes.

Feature requests and pull requests are also welcome.

## License

Licensed under either of [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0) or [MIT license](http://opensource.org/licenses/MIT) at your option.
