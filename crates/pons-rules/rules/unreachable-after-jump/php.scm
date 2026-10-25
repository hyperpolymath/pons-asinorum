(compound_statement (return_statement) @jump . (_) @next)
(compound_statement (break_statement) @jump . (_) @next)
(compound_statement (continue_statement) @jump . (_) @next)
(compound_statement (expression_statement (throw_expression) @jump) . (_) @next)
