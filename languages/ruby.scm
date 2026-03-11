; Ruby symbol extraction queries

; Methods
(method name: (identifier) @definition.method)
(singleton_method name: (identifier) @definition.method)

; Classes
(class name: (constant) @definition.class)

; Modules
(module name: (constant) @definition.module)

; Constants
(assignment left: (constant) @definition.constant)

; Attribute accessors
(call method: (identifier) @definition.variable
  (#match? @definition.variable "^attr_(reader|writer|accessor)$"))

; Requires
(call method: (identifier) @_req arguments: (argument_list (string) @reference.import)
  (#match? @_req "^require"))

; Method calls
(call method: (identifier) @reference.call)
