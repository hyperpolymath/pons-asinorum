// SPDX-License-Identifier: MPL-2.0

//! Control-flow graph construction for the T1 dataflow analyses (ADR-0002).
//!
//! Deliberately almost empty of builder code at this commit. The first thing
//! this module owes the rest of the crate is proof that the grammar vocabulary
//! it is about to be written against is the vocabulary the *pinned parser*
//! actually produces — see [`grammar_probe`]. Writing a CFG builder against
//! guessed node names is the same class of mistake as reading a config key with
//! `grep` and assuming the parser agrees.
//!
//! That probe has already overturned two claims this module would otherwise
//! have been built on; [`is_except_group`] exists because of one of them.

use tree_sitter::Node;

#[cfg(test)]
mod grammar_probe;

/// Is this `except_clause` a PEP 654 exception *group* handler (`except*`)?
///
/// **This cannot be left to the `has_error()` OPAQUE trigger.** ADR-0002's
/// implementation plan assumed `except*` is unparseable by the pinned grammar
/// and so would be caught as an error node. Measured, it is not:
/// `tree-sitter-python` 0.25.0 parses `except* ValueError:` to an ordinary
/// `except_clause` with the `*` demoted to an *anonymous* child, producing a
/// named-tree shape **identical** to plain `except ValueError:`.
/// `has_error()` returns `false`.
///
/// Silently misreading a group handler as an ordinary one is worse than
/// refusing to analyse it: the two have genuinely different control flow (a
/// group handler runs for the matching *part* of a raised group, and several
/// sibling handlers can each run for a single raise), so the builder would emit
/// a confident, plausible, wrong CFG and both T1 rules would report against it.
///
/// The `*` token does survive as an anonymous child, so the discrimination is
/// structural rather than textual.
// Consumed by the CFG builder in Step 2 (the OPAQUE hatch); until that lands the
// grammar probe is its only caller, which reads as dead code in a non-test build.
#[allow(dead_code)]
pub(crate) fn is_except_group(except_clause: Node<'_>) -> bool {
    debug_assert_eq!(except_clause.kind(), "except_clause");
    let mut cursor = except_clause.walk();
    for child in except_clause.children(&mut cursor) {
        if !child.is_named() && child.kind() == "*" {
            return true;
        }
    }
    false
}
