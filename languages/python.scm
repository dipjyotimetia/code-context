; Python symbol extraction queries

; Functions
(function_definition name: (identifier) @definition.function)

; Classes
(class_definition name: (identifier) @definition.class)

; Methods
(class_definition
  body: (block
    (function_definition name: (identifier) @definition.method)))

; Decorators
(decorated_definition
  (decorator) @reference.decorator
  definition: (function_definition name: (identifier) @definition.function))
(decorated_definition
  (decorator) @reference.decorator
  definition: (class_definition name: (identifier) @definition.class))

; Assignments (top-level constants/variables)
(module
  (expression_statement
    (assignment left: (identifier) @definition.variable)))

; Imports
(import_statement name: (dotted_name) @reference.import)
(import_from_statement
  module_name: (dotted_name) @reference.import)
(import_from_statement
  name: (dotted_name) @reference.import)

; Function calls
(call function: (identifier) @reference.call)
(call function: (attribute attribute: (identifier) @reference.call))
