; TypeScript/JavaScript symbol extraction queries

; Functions
(function_declaration name: (identifier) @definition.function)
(generator_function_declaration name: (identifier) @definition.function)

; Arrow functions assigned to variables
(lexical_declaration
  (variable_declarator
    name: (identifier) @definition.function
    value: (arrow_function)))
(variable_declaration
  (variable_declarator
    name: (identifier) @definition.function
    value: (arrow_function)))

; Classes
(class_declaration name: (identifier) @definition.class)

; Methods
(method_definition name: (property_identifier) @definition.method)

; Interfaces (TS only)
(interface_declaration name: (identifier) @definition.interface)

; Type aliases (TS only)
(type_alias_declaration name: (identifier) @definition.type)

; Enums (TS only)
(enum_declaration name: (identifier) @definition.enum)

; Variable declarations
(lexical_declaration
  (variable_declarator name: (identifier) @definition.variable))
(variable_declaration
  (variable_declarator name: (identifier) @definition.variable))

; Exports
(export_statement
  declaration: (function_declaration name: (identifier) @definition.function))
(export_statement
  declaration: (class_declaration name: (identifier) @definition.class))

; Imports
(import_statement source: (string) @reference.import)

; Function calls
(call_expression function: (identifier) @reference.call)
(call_expression function: (member_expression property: (property_identifier) @reference.call))
