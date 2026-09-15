; SPDX-License-Identifier: MPL-2.0

(binary_operator
  left: (_) @lhs
  operator: "/" @op
  right: [(integer) (float)] @rhs) @expr

(binary_operator
  left: (_) @lhs
  operator: "%" @op
  right: [(integer) (float)] @rhs) @expr

(augmented_assignment
  left: (_) @lhs
  operator: "/=" @op
  right: [(integer) (float)] @rhs) @expr

(augmented_assignment
  left: (_) @lhs
  operator: "%=" @op
  right: [(integer) (float)] @rhs) @expr
