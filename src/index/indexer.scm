(use_statement
  (file_name) @name
  (#set! index.element file_dependency)) @reference.file_dependency

(class_definition
  (class_header
    name: (identifier) @name) @definition.class
  (#set! index.element class_definition))

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
