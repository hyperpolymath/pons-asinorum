(augmented_assignment
  left: (identifier) @target
  right: (_) @addend) @assign

(assignment
  left: (identifier) @target
  right: (binary_operator
    left: (identifier) @addend_base
    right: (_) @addend) @binop) @assign
