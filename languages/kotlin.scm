; Kotlin symbol extraction queries

; Functions
(function_declaration (simple_identifier) @definition.function)

; Classes
(class_declaration (type_identifier) @definition.class)

; Objects
(object_declaration (type_identifier) @definition.class)

; Interfaces
(class_declaration (type_identifier) @definition.interface)

; Enums
(class_declaration (type_identifier) @definition.enum)

; Properties
(property_declaration (variable_declaration (simple_identifier) @definition.variable))

; Type aliases
(type_alias (type_identifier) @definition.type)

; Imports
(import_header (identifier) @reference.import)

; Function calls
(call_expression (simple_identifier) @reference.call)
(call_expression (navigation_expression (simple_identifier) @reference.call))
