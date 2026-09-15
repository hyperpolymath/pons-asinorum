; SPDX-License-Identifier: MPL-2.0

(binary_expression
  left: (_) @lhs
  operator: "/" @op
  right: (number) @rhs) @expr

(binary_expression
  left: (_) @lhs
  operator: "%" @op
  right: (number) @rhs) @expr

(augmented_assignment_expression
  left: (_) @lhs
  operator: "/=" @op
  right: (number) @rhs) @expr

(augmented_assignment_expression
  left: (_) @lhs
  operator: "%=" @op
  right: (number) @rhs) @expr
