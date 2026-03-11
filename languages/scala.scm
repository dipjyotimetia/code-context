; Scala symbol extraction queries

; Classes
(class_definition name: (identifier) @definition.class)

; Objects
(object_definition name: (identifier) @definition.class)

; Traits
(trait_definition name: (identifier) @definition.trait)

; Functions/Methods
(function_definition name: (identifier) @definition.method)

; Value definitions
(val_definition pattern: (identifier) @definition.variable)

; Variable definitions
(var_definition pattern: (identifier) @definition.variable)

; Type definitions
(type_definition name: (type_identifier) @definition.type)

; Imports
(import_declaration (import_expression) @reference.import)

; Function calls
(call_expression function: (identifier) @reference.call)
