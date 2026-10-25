// SPDX-License-Identifier: MPL-2.0
fn has_cycle(visited: bool, recurse: bool, back_edge: bool) -> bool {
    if !visited {
        if recurse {
            return true;
        }
    } else if back_edge {
        return true; // Back edge found - cycle detected
    }
    false
}
