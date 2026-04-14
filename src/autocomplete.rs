//! This module provides the functionality to determine which nodes are suitable for connecting to a certain port.
//! It can be used to tell users which nodes they can use to connect to a certain port.
use crate::{
    inference::infer,
    nodety::inference::{Flow, InferenceConfig},
    scope::{Scope, ScopePointer},
    r#type::Type,
    type_expr::{ScopePortal, ScopedTypeExpr, TypeExpr, node_signature::NodeSignature},
};
use std::collections::BTreeMap;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// Which side of a connection is being completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum Side {
    /// Completing for an input port — finding outputs that can connect to it.
    Input,
    /// Completing for an output port — finding inputs that can connect from it.
    Output,
}

/// A single autocompletion candidate: a port on a node signature that is compatible.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
pub struct Autocompletion<I> {
    /// Identifier of the node signature.
    pub signature_ident: I,
    /// Index of the compatible port.
    pub port_idx: usize,
}

/// Collects node signatures and finds compatible connection targets.
///
/// Generic over `T` (the type system) and `I` (the identifier type for signatures, e.g. `i32` for wasm).
#[derive(Debug, Clone, Default)]
pub struct Autocomplete<T: Type, I: Ord + Clone> {
    available_signatures: BTreeMap<I, NodeSignature<T, ScopePortal<T>>>,
}

/// Tries to rate a target as to how likely it is to be a good match for any source.
/// Implemented on best effort basis.
fn rank_autocompletion_target<T: Type>(target: &ScopedTypeExpr<T>, scope: &ScopePointer<T>) -> i16 {
    if target.is_any(scope).unwrap_or(false) {
        // Any is a bad target because it accepts everything
        return -100;
    }
    let mut score = 0;
    target.traverse(
        scope,
        &mut |expr, scope, _is_tl_union| {
            match expr {
                // A specific type is probably a good target
                TypeExpr::Type(_) => score += 1,

                // For type parameters, rate their bound
                TypeExpr::TypeParameter(param, _infer) => {
                    let Some((bound, bound_scope)) = scope.lookup_bound(param) else {
                        score -= 1;
                        return;
                    };
                    score += rank_autocompletion_target(&bound, &bound_scope) - 1;
                }
                _ => (),
            }
        },
        true,
    );
    score
}

impl<T: Type, I: Ord + Clone> Autocomplete<T, I> {
    /// Creates an empty autocomplete context.
    pub fn new() -> Self {
        Self { available_signatures: BTreeMap::new() }
    }

    /// Adds a node signature with the given identifier.
    pub fn add_signature(&mut self, identifier: I, signature: impl Into<NodeSignature<T, ScopePortal<T>>>) {
        self.available_signatures.insert(identifier, signature.into());
    }

    /// Generates potential candidates that can connect to the given type expression.
    /// Results are scored and sorted using [rank_autocompletion_target].
    ///
    /// - **Input**: `expr` is the type of an input port; returns outputs from other nodes that can connect to it.
    /// - **Output**: `expr` is the type of an output port; returns inputs from other nodes that can connect from it.
    pub fn autocomplete(&self, completing_for: Side, expr: impl Into<ScopedTypeExpr<T>>) -> Vec<Autocompletion<I>> {
        let expr: ScopedTypeExpr<T> = expr.into();
        let expr_scope = Scope::new_root();

        // (Autocompletion, score)
        let mut autocompletions: Vec<(Autocompletion<I>, i16)> = Vec::new();

        for (ident, sig) in &self.available_signatures {
            let test_ports = match completing_for {
                Side::Input => &sig.outputs,
                Side::Output => &sig.inputs,
            };
            let TypeExpr::PortTypes(test_ports) = test_ports else { continue };

            let mut scope = Scope::new_root();
            for (param_ident, param) in &sig.parameters {
                scope.define(*param_ident, param.clone());
            }

            let test_port_count = test_ports.ports.len() + test_ports.varg.is_some() as usize;
            for test_port_idx in 0..test_port_count {
                let Some(port_type) = test_ports.get_port_type(test_port_idx) else { continue };
                let port_type: ScopedTypeExpr<T> = port_type.clone();

                let compatible = match completing_for {
                    Side::Input => is_compatible(&port_type, &expr, scope.clone(), expr_scope.clone()),
                    Side::Output => is_compatible(&expr, &port_type, expr_scope.clone(), scope.clone()),
                };

                if compatible {
                    let score = rank_autocompletion_target(&port_type, &ScopePointer::new(scope.clone()));

                    autocompletions
                        .push((Autocompletion { signature_ident: ident.clone(), port_idx: test_port_idx }, score));
                }
            }
        }

        autocompletions.sort_by_key(|(_, score)| *score);

        autocompletions.into_iter().map(|(autocompletion, _)| autocompletion).collect()
    }
}

/// Determines whether or not a connection between an input and output port is valid.
pub fn is_compatible<T: Type>(
    output: &ScopedTypeExpr<T>,
    input: &ScopedTypeExpr<T>,
    output_scope: Scope<T>,
    input_scope: Scope<T>,
) -> bool {
    let output_scope_pointer = ScopePointer::new(output_scope);
    let input_scope_pointer = ScopePointer::new(input_scope);

    let flows = vec![Flow {
        source: output.clone(),
        target: input.clone(),
        source_scope: ScopePointer::clone(&output_scope_pointer),
        target_scope: ScopePointer::clone(&input_scope_pointer),
    }];

    infer(flows, &InferenceConfig::default());

    output_scope_pointer.infer_defaults();
    input_scope_pointer.infer_defaults();

    input.supertype_of(output, &input_scope_pointer, &output_scope_pointer).is_supertype()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::parse::{expr, scope, sig};

    #[test]
    fn test_is_compatible() {
        assert!(is_compatible(&expr("Any"), &expr("Any"), scope("<>"), scope("<>")));

        assert!(is_compatible(&expr("Integer"), &expr("Any"), scope("<>"), scope("<>")));

        assert!(!is_compatible(&expr("Any"), &expr("Integer"), scope("<>"), scope("<>")));

        assert!(is_compatible(&expr("T"), &expr("T"), scope("<T>"), scope("<T>")));

        assert!(is_compatible(&expr("Array<T>"), &expr("T"), scope("<T>"), scope("<T>")));

        assert!(is_compatible(&expr("Array<T>"), &expr("Array<T>"), scope("<T>"), scope("<T>")));

        assert!(!is_compatible(
            &expr("T"),
            &expr("T"),
            scope("<T extends Array<Integer>>"),
            scope("<T extends Integer>"),
        ));
    }

    #[test]
    fn test_autocomplete() {
        let mut autocomplete = Autocomplete::<crate::demo_type::DemoType, i32>::new();
        autocomplete.add_signature(1, sig("(Integer) -> (Float)"));
        autocomplete.add_signature(2, sig("(String) -> (Integer)"));
        autocomplete.add_signature(3, sig("<T>(Array<T>) -> ()"));
        autocomplete.add_signature(4, sig("(Array<Integer>) -> ()"));

        let completions = autocomplete.autocomplete(Side::Input, expr("Integer"));
        assert_eq!(completions, vec![Autocompletion { signature_ident: 2, port_idx: 0 }]);

        let completions = autocomplete.autocomplete(Side::Output, expr("Integer"));
        assert_eq!(completions, vec![Autocompletion { signature_ident: 1, port_idx: 0 }]);

        let completions = autocomplete.autocomplete(Side::Output, expr("Array<{a: Integer}>"));
        assert_eq!(completions, vec![Autocompletion { signature_ident: 3, port_idx: 0 }]);

        let completions = autocomplete.autocomplete(Side::Output, expr("Array<Integer>"));
        assert_eq!(
            completions,
            vec![
                Autocompletion { signature_ident: 3, port_idx: 0 },
                Autocompletion { signature_ident: 4, port_idx: 0 },
            ]
        );
    }
}
