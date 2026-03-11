; Bash symbol extraction queries

; Functions
(function_definition name: (word) @definition.function)

; Variable assignments
(variable_assignment name: (variable_name) @definition.variable)

; Command calls
(command name: (command_name) @reference.call)

; Source/include
(command name: (command_name) @_cmd argument: (word) @reference.import
  (#match? @_cmd "^(source|\\.)$"))
