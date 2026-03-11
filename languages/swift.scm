; Swift symbol extraction queries

; Functions
(function_declaration name: (simple_identifier) @definition.function)

; Classes
(class_declaration name: (type_identifier) @definition.class)

; Structs
(class_declaration name: (type_identifier) @definition.struct)

; Protocols
(protocol_declaration name: (type_identifier) @definition.interface)

; Enums
(enum_declaration name: (type_identifier) @definition.enum)

; Typealiases
(typealias_declaration name: (type_identifier) @definition.type)

; Properties
(property_declaration (pattern (simple_identifier) @definition.variable))

; Methods
(function_declaration name: (simple_identifier) @definition.method)

; Imports
(import_declaration (identifier) @reference.import)

; Function calls
(call_expression (simple_identifier) @reference.call)
