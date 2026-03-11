; Go symbol extraction queries

; Functions
(function_declaration name: (identifier) @definition.function)

; Methods
(method_declaration name: (field_identifier) @definition.method)

; Structs
(type_declaration (type_spec name: (type_identifier) @definition.struct type: (struct_type)))

; Interfaces
(type_declaration (type_spec name: (type_identifier) @definition.interface type: (interface_type)))

; Type aliases
(type_declaration (type_spec name: (type_identifier) @definition.type))

; Constants
(const_declaration (const_spec name: (identifier) @definition.constant))

; Variables
(var_declaration (var_spec name: (identifier) @definition.variable))

; Package
(package_clause (package_identifier) @definition.module)

; Imports
(import_spec path: (interpreted_string_literal) @reference.import)

; Function calls
(call_expression function: (identifier) @reference.call)
(call_expression function: (selector_expression field: (field_identifier) @reference.call))
