; HCL/Terraform symbol extraction queries

; Resources
(block (identifier) @_type (string_lit) @_rtype (string_lit) @definition.variable
  (#match? @_type "^resource$"))

; Data sources
(block (identifier) @_type (string_lit) @_dtype (string_lit) @definition.variable
  (#match? @_type "^data$"))

; Variables
(block (identifier) @_type (string_lit) @definition.variable
  (#match? @_type "^variable$"))

; Outputs
(block (identifier) @_type (string_lit) @definition.variable
  (#match? @_type "^output$"))

; Modules
(block (identifier) @_type (string_lit) @definition.module
  (#match? @_type "^module$"))

; Locals
(block (identifier) @_type
  (#match? @_type "^locals$"))
