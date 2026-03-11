; C++ symbol extraction queries

; Functions
(function_definition declarator: (function_declarator declarator: (identifier) @definition.function))
(function_definition declarator: (function_declarator declarator: (qualified_identifier name: (identifier) @definition.method)))

; Classes
(class_specifier name: (type_identifier) @definition.class)

; Structs
(struct_specifier name: (type_identifier) @definition.struct)

; Enums
(enum_specifier name: (type_identifier) @definition.enum)

; Namespaces
(namespace_definition name: (identifier) @definition.module)

; Templates
(template_declaration
  (function_definition declarator: (function_declarator declarator: (identifier) @definition.function)))
(template_declaration
  (class_specifier name: (type_identifier) @definition.class))

; Typedefs
(type_definition declarator: (type_identifier) @definition.type)
(alias_declaration name: (type_identifier) @definition.type)

; Macros
(preproc_function_def name: (identifier) @definition.macro)
(preproc_def name: (identifier) @definition.macro)

; Includes
(preproc_include path: (_) @reference.import)

; Function calls
(call_expression function: (identifier) @reference.call)
(call_expression function: (qualified_identifier name: (identifier) @reference.call))
(call_expression function: (field_expression field: (field_identifier) @reference.call))
