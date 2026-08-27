# Todos

- Add TypeExpr::Ref. Then move TypeExpr::TypeParameter and TypeExpr::ScopePortal into that so that the type system can represent
  non generic types, generic types, scoped generic types, etc.
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
