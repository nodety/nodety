# Todos

- Functions that take `Option<ConstructorParams>` should instead take `ConstructorParams` where None is `ConstructorParams::default()`
- Remove petgraph dependency
- Evaluate if keyof T and T[K] can be represented as operators
- Mapped types
- Build the official website with showcase
- Add more tests for keyof
- Add more tests for index
- Extract parameters from signature
- Test tags
- Test inferring from and to conditionals
- Test cyclic graphs
- Enable traverse return early?

# Proptest

- Add proptest that a random type is supertype of itself
- Proptest normalization function.
- Add proptests to dedoup scope portals. (same semantics as non dedouped (supertyping))
