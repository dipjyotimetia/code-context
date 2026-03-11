; Java symbol extraction queries

; Classes
(class_declaration name: (identifier) @definition.class)

; Interfaces
(interface_declaration name: (identifier) @definition.interface)

; Enums
(enum_declaration name: (identifier) @definition.enum)

; Methods
(method_declaration name: (identifier) @definition.method)

; Constructors
(constructor_declaration name: (identifier) @definition.method)

; Fields
(field_declaration declarator: (variable_declarator name: (identifier) @definition.variable))

; Constants
(constant_declaration declarator: (variable_declarator name: (identifier) @definition.constant))

; Annotations
(annotation name: (identifier) @reference.decorator)

; Imports
(import_declaration (scoped_identifier) @reference.import)

; Method calls
(method_invocation name: (identifier) @reference.call)

; Type references
(type_identifier) @reference.type
