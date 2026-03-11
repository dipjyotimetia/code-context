; Rust symbol extraction queries

; Functions
(function_item name: (identifier) @definition.function)
(function_signature_item name: (identifier) @definition.function)

; Methods (impl blocks)
(impl_item
  body: (declaration_list
    (function_item name: (identifier) @definition.method)))

; Structs
(struct_item name: (type_identifier) @definition.struct)

; Enums
(enum_item name: (type_identifier) @definition.enum)

; Traits
(trait_item name: (type_identifier) @definition.trait)

; Type aliases
(type_item name: (type_identifier) @definition.type)

; Constants
(const_item name: (identifier) @definition.constant)
(static_item name: (identifier) @definition.constant)

; Modules
(mod_item name: (identifier) @definition.module)

; Impl blocks
(impl_item type: (type_identifier) @definition.impl)

; Macros
(macro_definition name: (identifier) @definition.macro)

; Use statements (imports)
(use_declaration argument: (_) @reference.import)

; Function calls
(call_expression function: (identifier) @reference.call)
(call_expression function: (field_expression field: (field_identifier) @reference.call))
(call_expression function: (scoped_identifier name: (identifier) @reference.call))

; Type references
(type_identifier) @reference.type
