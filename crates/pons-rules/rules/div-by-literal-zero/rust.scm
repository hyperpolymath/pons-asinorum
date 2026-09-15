; SPDX-License-Identifier: MPL-2.0

(binary_expression
  left: (_) @lhs
  operator: "/" @op
  right: [(integer_literal) (float_literal)] @rhs) @expr

(binary_expression
  left: (_) @lhs
  operator: "%" @op
  right: [(integer_literal) (float_literal)] @rhs) @expr

(compound_assignment_expr
  left: (_) @lhs
  operator: "/=" @op
  right: [(integer_literal) (float_literal)] @rhs) @expr

(compound_assignment_expr
  left: (_) @lhs
  operator: "%=" @op
  right: [(integer_literal) (float_literal)] @rhs) @expr
