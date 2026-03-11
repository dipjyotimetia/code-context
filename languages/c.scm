; C symbol extraction queries

; Functions
(function_definition declarator: (function_declarator declarator: (identifier) @definition.function))
(declaration declarator: (function_declarator declarator: (identifier) @definition.function))

; Structs
(struct_specifier name: (type_identifier) @definition.struct)

; Enums
(enum_specifier name: (type_identifier) @definition.enum)

; Typedefs
(type_definition declarator: (type_identifier) @definition.type)

; Macros
(preproc_function_def name: (identifier) @definition.macro)
(preproc_def name: (identifier) @definition.macro)

; Global variables
(declaration declarator: (init_declarator declarator: (identifier) @definition.variable))

; Includes
(preproc_include path: (_) @reference.import)

; Function calls
(call_expression function: (identifier) @reference.call)
