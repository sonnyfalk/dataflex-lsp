(use_statement
  (file_name) @name
  (#set! index.element file_dependency)) @element_node

(class_definition
  (class_header
    name: (identifier) @name
    superclass: (identifier) @superclass) 
  (#set! index.element class_definition)) @element_node

(class_definition
  (class_footer) 
  (#set! index.element pop_stack_symbol)) @element_node

(composite_definition
  (composite_header
    name: (identifier) @name
    superclass: (identifier) @superclass) 
  (#set! index.element class_definition)) @element_node

(composite_definition
  (composite_footer)
  (#set! index.element pop_stack_symbol)) @element_node

(object_definition
  (object_header
    name: (identifier) @name
    superclass: (identifier) @superclass)
  (#set! index.element object_definition)) @element_node

(object_definition
  (object_footer)
  (#set! index.element pop_stack_symbol)) @element_node

(class_definition
  (procedure_definition
    (procedure_header
      name: (identifier) @name
      (parameter)* @parameter)
    (#set! index.element method_procedure_definition)) @element_node)

(class_definition
  (function_definition
    (function_header
      name: (identifier) @name
      (parameter)* @parameter
      return_type: (typedecl) @return_type)
    (#set! index.element method_function_definition)) @element_node)

(object_definition
  (procedure_definition
    (procedure_header
      name: (identifier) @name
      (parameter)* @parameter)
    (#set! index.element method_procedure_definition)) @element_node)

(object_definition
  (function_definition
    (function_header
      name: (identifier) @name
      (parameter)* @parameter
      return_type: (typedecl) @return_type)
    (#set! index.element method_function_definition)) @element_node)

(property_definition
  type: [
    (system_typedecl
      (system_type) @type
      (array_decl)* @array)
    (custom_typedecl
      (identifier) @type
      (array_decl)* @array)
  ]
  name: (identifier) @name
  (#set! index.element property_definition)) @element_node

(struct_declaration
  (struct_header
    name: (identifier) @name)
  (#set! index.element struct_declaration)) @element_node

(struct_declaration
  (struct_footer)
  (#set! index.element pop_stack_symbol)) @element_node

(struct_declaration
  (struct_member
    [
      (system_typedecl
        (system_type) @type
        (array_decl)* @array)
      (custom_typedecl
        (identifier) @type
        (array_decl)* @array)
    ]
    (identifier) @name)
  (#set! index.element struct_member)) @element_node

(global_variable_declaration
  [
    (system_typedecl
      (system_type) @type
      (array_decl)* @array)
    (custom_typedecl
      (identifier) @type
      (array_decl)* @array)
  ]
  (identifier) @name
  (#set! index.element global_variable_declaration)) @element_node

(define_declaration
  name: (identifier) @name
  value: [
    (identifier) @name_reference
    (number_literal) @value_reference
    (string_literal) @value_reference
    (paren_expression) @value_reference
  ]?
  (#set! index.element alias_definition)) @element_node

(replace_declaration
  name: (expression) @name
  value: [
    (identifier) @name_reference
    (number_literal) @value_reference
    (string_literal) @value_reference
    (paren_expression) @value_reference
    (icode_argument) @arg_reference
  ]
  (#set! index.element alias_definition)) @element_node

(mixin_class
  name: (identifier) @name
  (#set! index.element mixin_class)) @element_node
