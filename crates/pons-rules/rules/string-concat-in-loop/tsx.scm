(augmented_assignment_expression
  left: (identifier) @target
  right: (_) @addend) @assign

(assignment_expression
  left: (identifier) @target
  right: (binary_expression
    left: (identifier) @addend_base
    right: (_) @addend) @binop) @assign
