(use_statement
  (file_name) @name
  (#set! index.element file_dependency)) @reference.file_dependency

(class_definition
  (class_header
    name: (identifier) @name
    superclass: (identifier) @superclass) @definition.class
  (#set! index.element class_definition))

(class_definition
  (class_footer) @definition.class
  (#set! index.element pop_stack_symbol))

(object_definition
  (object_header
    name: (identifier) @name
    superclass: (identifier) @superclass) @definition.object
  (#set! index.element object_definition))

(object_definition
  (object_footer) @definition.object
  (#set! index.element pop_stack_symbol))

(class_definition
  (procedure_definition
    (procedure_header
      name: (identifier) @name) @definition.method
    (#set! index.element method_procedure_definition)))

(class_definition
  (function_definition
    (function_header
      name: (identifier) @name) @definition.method
    (#set! index.element method_function_definition)))

(object_definition
  (procedure_definition
    (procedure_header
      name: (identifier) @name) @definition.method
    (#set! index.element method_procedure_definition)))

(object_definition
  (function_definition
    (function_header
      name: (identifier) @name) @definition.method
    (#set! index.element method_function_definition)))

(property_definition
  name: (identifier) @name
  (#set! index.element property_definition)) @definition.property

(struct_declaration
  (struct_header
    name: (identifier) @name) @definition.struct
  (#set! index.element struct_declaration))

(struct_declaration
  (struct_footer) @definition.struct
  (#set! index.element pop_stack_symbol))

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
  (#set! index.element struct_member)) @definition.struct_member

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
  (#set! index.element global_variable_declaration)) @definition.variable.global
