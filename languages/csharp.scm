; C# symbol extraction queries

; Classes
(class_declaration name: (identifier) @definition.class)

; Interfaces
(interface_declaration name: (identifier) @definition.interface)

; Enums
(enum_declaration name: (identifier) @definition.enum)

; Structs
(struct_declaration name: (identifier) @definition.struct)

; Records
(record_declaration name: (identifier) @definition.class)

; Methods
(method_declaration name: (identifier) @definition.method)

; Constructors
(constructor_declaration name: (identifier) @definition.method)

; Properties
(property_declaration name: (identifier) @definition.variable)

; Fields
(field_declaration (variable_declaration (variable_declarator (identifier) @definition.variable)))

; Delegates
(delegate_declaration name: (identifier) @definition.type)

; Namespaces
(namespace_declaration name: (_) @definition.module)

; Using statements
(using_directive (_) @reference.import)

; Method calls
(invocation_expression function: (member_access_expression name: (identifier) @reference.call))
(invocation_expression function: (identifier) @reference.call)
