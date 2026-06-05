use super::*;
use std::collections::HashSet;

use index::{ClassSymbol, IndexSymbolType, MethodKind, SymbolName};

#[derive(Debug)]
pub struct CodeLens {
    pub location: Point,
    pub description: String,
}

impl CodeLens {
    pub fn code_lens(doc: &DataFlexDocument) -> Vec<CodeLens> {
        let Some(root_node) = doc.root_node() else {
            return Vec::new();
        };

        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            r#"
            (class_definition
                (class_header
                    superclass: (identifier) @superclass)
                [
                (procedure_definition
                    (procedure_header
                        !set
                        name: (identifier) @method.msg.name)) @method.msg
                (function_definition
                    (function_header
                        name: (identifier) @method.get.name)) @method.get
                (procedure_definition
                    (procedure_header
                        set: (keyword)
                        name: (identifier) @method.set.name)) @method.set
                (_)
                ]*
            )

            (object_definition
                (object_header
                    superclass: (identifier) @superclass)
                [
                (procedure_definition
                    (procedure_header
                        !set
                        name: (identifier) @method.msg.name)) @method.msg
                (function_definition
                    (function_header
                        name: (identifier) @method.get.name)) @method.get
                (procedure_definition
                    (procedure_header
                        set: (keyword)
                        name: (identifier) @method.set.name)) @method.set
                (_)
                ]*
            )

            (composite_definition
                (composite_header
                    superclass: (identifier) @superclass)
                [
                (procedure_definition
                    (procedure_header
                        !set
                        name: (identifier) @method.msg.name)) @method.msg
                (function_definition
                    (function_header
                        name: (identifier) @method.get.name)) @method.get
                (procedure_definition
                    (procedure_header
                        set: (keyword)
                        name: (identifier) @method.set.name)) @method.set
                (_)
                ]*
            )
            "#,
        )
        .expect("Error loading code lens query");

        let superclass_capture_index = query.capture_index_for_name("superclass").unwrap();
        let msg_capture_index = query.capture_index_for_name("method.msg").unwrap();
        let msg_name_capture_index = query.capture_index_for_name("method.msg.name").unwrap();
        let get_capture_index = query.capture_index_for_name("method.get").unwrap();
        let get_name_capture_index = query.capture_index_for_name("method.get.name").unwrap();
        let set_capture_index = query.capture_index_for_name("method.set").unwrap();
        let set_name_capture_index = query.capture_index_for_name("method.set.name").unwrap();

        let mut query_cursor = tree_sitter::QueryCursor::new();
        let matches = query_cursor.matches(&query, root_node, doc.line_map.text_provider());
        let index = doc.index.get();

        matches.fold(Vec::new(), |mut result, query_match| {
            let class_hierarchy: Vec<&SymbolName> = query_match
                .nodes_for_capture_index(superclass_capture_index)
                .next()
                .map(|n| SymbolName::from(doc.line_map.text_for_node(&n)))
                .and_then(|name| index.find_class(&name))
                .and_then(|symbol_ref| index.symbol_snapshot(symbol_ref))
                .and_then(|s| ClassSymbol::from_index_symbol(s.symbol))
                .iter()
                .flat_map(|c| index.class_hierarchy(c))
                .map(|c| c.symbol_path.name())
                .collect();
            let class_set: HashSet<_> = class_hierarchy.iter().collect();

            query_match
                .nodes_for_capture_index(msg_capture_index)
                .zip(query_match.nodes_for_capture_index(msg_name_capture_index))
                .map(|(method_node, name_node)| (method_node, name_node, MethodKind::Msg))
                .chain(
                    query_match
                        .nodes_for_capture_index(get_capture_index)
                        .zip(query_match.nodes_for_capture_index(get_name_capture_index))
                        .map(|(method_node, name_node)| (method_node, name_node, MethodKind::Get)),
                )
                .chain(
                    query_match
                        .nodes_for_capture_index(set_capture_index)
                        .zip(query_match.nodes_for_capture_index(set_name_capture_index))
                        .map(|(method_node, name_node)| (method_node, name_node, MethodKind::Set)),
                )
                .map(|(method_node, name_node, kind)| {
                    (
                        method_node.start_position(),
                        SymbolName::from(doc.line_map.text_for_node(&name_node)),
                        kind,
                    )
                })
                .filter_map(|(position, name, kind)| {
                    log::trace!("CodeLens candidate {name} at {position}");
                    let overrides: Vec<&SymbolName> = index
                        .find_methods(&name, kind)
                        .filter_map(|symbol_ref| symbol_ref.symbol_path.parent_name())
                        .filter(|class_name| class_set.contains(class_name))
                        .collect();
                    if overrides.len() > 1 {
                        class_hierarchy
                            .iter()
                            .find(|class| overrides.contains(class))
                            .map(|class| (position, format!("Overrides {name} in {class}")))
                    } else {
                        overrides
                            .first()
                            .map(|class| (position, format!("Overrides {name} in {class}")))
                    }
                })
                .for_each(|(position, text)| {
                    log::trace!("CodeLens {text} at {position}");
                    result.push(CodeLens {
                        location: position,
                        description: text,
                    });
                });
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_lens_with_class() {
        let test_content = r#"
Class cMyBaseClass is a cObject
    Procedure MyMethod
    End_Procedure
End_Object

Class cMySubClass is a cMyBaseClass {
    Procedure MyMethod
    End_Procedure
End_Class
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let result = CodeLens::code_lens(&doc);
        assert_eq!(
            format!("{:?}", result),
            "[CodeLens { location: Point { row: 7, column: 4 }, description: \"Overrides MyMethod in cMyBaseClass\" }]"
        );
    }

    #[test]
    fn test_code_lens_with_object() {
        let test_content = r#"
Class cMyClass is a cObject
    Procedure MyMethod
    End_Procedure
End_Object

Object oMyObject is a cMyClass {
    Procedure MyMethod
    End_Procedure
End_Object
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let result = CodeLens::code_lens(&doc);
        assert_eq!(
            format!("{:?}", result),
            "[CodeLens { location: Point { row: 7, column: 4 }, description: \"Overrides MyMethod in cMyClass\" }]"
        );
    }

    #[test]
    fn test_code_lens_with_composite() {
        let test_content = r#"
Class cMyClass is a cObject
    Procedure MyMethod
    End_Procedure
End_Object

Composite cMyComposite is a cMyClass {
    Procedure MyMethod
    End_Procedure
End_Composite
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let result = CodeLens::code_lens(&doc);
        assert_eq!(
            format!("{:?}", result),
            "[CodeLens { location: Point { row: 7, column: 4 }, description: \"Overrides MyMethod in cMyClass\" }]"
        );
    }
}
