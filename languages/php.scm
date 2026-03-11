; PHP symbol extraction queries

; Functions
(function_definition name: (name) @definition.function)

; Classes
(class_declaration name: (name) @definition.class)

; Interfaces
(interface_declaration name: (name) @definition.interface)

; Traits
(trait_declaration name: (name) @definition.trait)

; Methods
(method_declaration name: (name) @definition.method)

; Constants
(const_declaration (const_element (name) @definition.constant))

; Properties
(property_declaration (property_element (variable_name) @definition.variable))

; Enums
(enum_declaration name: (name) @definition.enum)

; Namespace
(namespace_definition name: (namespace_name) @definition.module)

; Use statements
(namespace_use_declaration (namespace_use_clause) @reference.import)

; Function calls
(function_call_expression function: (name) @reference.call)
(member_call_expression name: (name) @reference.call)
(scoped_call_expression name: (name) @reference.call)
