// SPDX-License-Identifier: MPL-2.0
// Reduced from wordpress-tools 80f4007 praxis validation.rs.
fn has_cycle(a: bool, b: bool) -> bool {
    if a { if b { return true; } } else if b {
        return true; // Back edge found - cycle detected
    }
    false
}
