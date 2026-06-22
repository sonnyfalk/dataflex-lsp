use std::path::PathBuf;

use tower_lsp::lsp_types;
use tree_sitter::{InputEdit, Point, Tree};

use crate::{dataflex_parser::DataFlexTreeParser, index};
use document_context::DocumentContext;
use reference_resolver::ReferenceResolver;
use streaming_iterator::StreamingIterator;
use tree_cursor::DataFlexTreeCursor;

mod code_completion;
mod code_lens;
mod document_context;
mod line_map;
mod parameter_info;
mod reference_resolver;
mod scope_balancer;
mod syntax_map;
mod tree_cursor;

#[allow(dead_code)]
pub struct DataFlexDocument {
    file_path: PathBuf,
    line_map: line_map::LineMap,
    parser: DataFlexTreeParser,
    index: index::IndexRef,
    tree: Option<Tree>,
    syntax_map: Option<syntax_map::SyntaxMap>,
}

impl DataFlexDocument {
    pub fn new(path: PathBuf, text: &str, index_ref: index::IndexRef) -> Self {
        let mut doc = Self {
            file_path: path,
            line_map: line_map::LineMap::new(text),
            parser: DataFlexTreeParser::new(),
            index: index_ref,
            tree: None,
            syntax_map: None,
        };
        doc.update();
        doc
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    pub fn root_node(&self) -> Option<tree_sitter::Node<'_>> {
        self.tree.as_ref().map(|tree| tree.root_node())
    }

    pub fn node_at_position(&self, position: Point) -> Option<tree_sitter::Node<'_>> {
        self.root_node()
            .and_then(|root_node| root_node.descendant_for_point_range(position, position))
    }

    pub fn symbol_at_position(&self, position: Point) -> Option<index::SymbolName> {
        self.node_at_position(position)
            .map(|node| self.line_map.text_for_node(&node).into())
    }

    fn local_variables(
        &self,
        position: Point,
    ) -> impl Iterator<Item = index::VariableSymbol> + use<'_> {
        let Some(method_node) = self
            .cursor()
            .and_then(|mut cursor| {
                cursor.goto_leaf_node_at_or_after_point(position).then(|| {
                    cursor
                        .goto_enclosing_method_definition()
                        .then(|| cursor.node())
                })
            })
            .flatten()
        else {
            return Vec::new().into_iter();
        };

        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            r#"
            (parameter
              [
                (system_typedecl
                  (system_type) @type
                  (array_decl)* @array_decl)
                (custom_typedecl
                  (identifier) @type
                  (array_decl)* @array_decl)
              ]
              name: (identifier)+ @name)

            (variable_declaration
              (system_typedecl
                (system_type) @type
                (array_decl)* @array)
              (identifier)+ @name)

            (potential_variable_declaration
              (custom_typedecl
                (identifier) @type
                (array_decl)* @array)
              (identifier)+ @name)
            "#,
        )
        .expect("Error loading local variables query");

        let name_capture_index = query.capture_index_for_name("name").unwrap();
        let type_capture_index = query.capture_index_for_name("type").unwrap();
        let array_capture_index = query.capture_index_for_name("array").unwrap();

        let mut query_cursor = tree_sitter::QueryCursor::new();
        let matches = query_cursor.matches(&query, method_node, self.line_map.text_provider());

        let vars: Vec<index::VariableSymbol> = matches.fold(Vec::new(), |mut vars, query_match| {
            if let Some(type_node) = query_match
                .nodes_for_capture_index(type_capture_index)
                .next()
            {
                let variable_type = self.line_map.text_for_node(&type_node);
                if type_node.kind() != "system_type"
                    && !self
                        .index
                        .get()
                        .is_known_struct(&variable_type.clone().into())
                {
                    return vars;
                }

                let array_dimension_count = query_match
                    .nodes_for_capture_index(array_capture_index)
                    .count();
                for name_node in query_match.nodes_for_capture_index(name_capture_index) {
                    let variable_name = self.line_map.text_for_node(&name_node);
                    let variable_type = if array_dimension_count == 0 {
                        index::DataFlexDataType::Simple(variable_type.clone().into())
                    } else {
                        index::DataFlexDataType::Array(
                            variable_type.clone().into(),
                            array_dimension_count,
                        )
                    };
                    vars.push(index::VariableSymbol {
                        location: name_node.start_position().into(),
                        range: name_node.range().into(),
                        symbol_path: index::SymbolPath::with_name(variable_name),
                        data_type: variable_type,
                        metadata: Vec::new(),
                    });
                }
            }
            vars
        });
        vars.into_iter()
    }

    fn find_local_variable(
        &self,
        position: Point,
        name: &index::SymbolName,
    ) -> Option<index::VariableSymbol> {
        self.local_variables(position)
            .find(|variable| variable.symbol_path.name() == name)
    }

    pub fn text_content(&self) -> String {
        self.line_map.text()
    }

    fn update(&mut self) {
        self.tree = self.parser.parse_with_options(
            &mut |_, point| {
                self.line_map
                    .line_text_with_ending(point.row)
                    .and_then(|line| line.as_bytes().get(point.column..))
                    .unwrap_or(&[])
            },
            self.tree.as_ref(),
            None,
        );
        self.update_syntax_map();
    }

    pub fn update_syntax_map(&mut self) {
        self.syntax_map = Some(syntax_map::SyntaxMap::new(self));
    }

    #[cfg(test)]
    pub fn replace_content(&mut self, text: &str) {
        self.line_map = line_map::LineMap::new(text);
        self.tree = None;
        self.update();
    }

    pub fn edit_content(
        &mut self,
        changes: &Vec<lsp_types::TextDocumentContentChangeEvent>,
    ) -> Option<Vec<lsp_types::TextEdit>> {
        for change in changes {
            let Some(range) = change.range else {
                self.line_map = line_map::LineMap::new(&change.text);
                self.tree = None;
                continue;
            };
            // TODO: Convert UTF-16 to UTF-8 range.
            let start = Point {
                row: range.start.line as usize,
                column: range.start.character as usize,
            };
            let end = Point {
                row: range.end.line as usize,
                column: range.end.character as usize,
            };
            let start_byte = self.line_map.offset_at_point(start);
            let old_end_byte = self.line_map.offset_at_point(end);
            let new_end_byte = start_byte + change.text.len();

            self.line_map.replace_range(start, end, &change.text);
            let new_end_position = self.line_map.point_at_offset(new_end_byte);

            if let Some(tree) = self.tree.as_mut() {
                tree.edit(&InputEdit {
                    start_byte,
                    old_end_byte,
                    new_end_byte,
                    start_position: start,
                    old_end_position: end,
                    new_end_position,
                });
            }
        }
        self.update();

        if changes.len() == 1
            && let Some(change) = changes.first()
            && scope_balancer::ScopeBalancer::is_auto_close_scope_trigger(&change.text)
            && let Some(position) = change
                .range
                .map(|range| Point::new(range.end.line as usize, range.end.character as usize))
        {
            scope_balancer::ScopeBalancer::auto_close_scope(self, position, &change.text).map(
                |edit| {
                    let start = lsp_types::Position::new(
                        edit.range.start.row as u32,
                        edit.range.start.column as u32,
                    );
                    let end = lsp_types::Position::new(
                        edit.range.end.row as u32,
                        edit.range.end.column as u32,
                    );
                    vec![lsp_types::TextEdit {
                        range: lsp_types::Range::new(start, end),
                        new_text: edit.text,
                    }]
                },
            )
        } else {
            None
        }
    }

    pub fn semantic_tokens_full(&self) -> Option<Vec<lsp_types::SemanticToken>> {
        let syntax_map = self.syntax_map.as_ref()?;
        Some(syntax_map.get_all_tokens())
    }

    pub fn find_definition(
        &self,
        position: lsp_types::Position,
    ) -> Option<Vec<lsp_types::Location>> {
        log::trace!("find_definition {:?}", position);
        let position = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        let Some(context) = DocumentContext::context(self, position) else {
            log::trace!("no context");
            return None;
        };
        log::trace!("context {:?}", context);

        let locations = if context.can_reference_variables()
            && let Some(symbol_name) = self.symbol_at_position(position)
            && let Some(variable) = self.find_local_variable(position, &symbol_name)
        {
            vec![lsp_types::Location::from(&index::IndexSymbolSnapshot {
                path: &self.file_path,
                symbol: &index::IndexSymbol::Variable(variable),
            })]
        } else if context.is_file_reference()
            && let Some(file_ref) = self
                .node_at_position(position)
                .map(|node| self.line_map.text_for_node(&node).into())
        {
            self.index
                .get()
                .find_file_path(&file_ref)
                .map(|path| {
                    lsp_types::Location::new(
                        lsp_types::Url::from_file_path(path).unwrap(),
                        lsp_types::Range::default(),
                    )
                })
                .into_iter()
                .collect()
        } else {
            let reference_resolver = ReferenceResolver::new(self);
            let symbols = reference_resolver.resolve_reference(context, position);
            symbols
                .map(|symbol_snapshot| lsp_types::Location::from(&symbol_snapshot))
                .collect()
        };

        if !locations.is_empty() {
            Some(locations)
        } else {
            None
        }
    }

    pub fn code_completion(
        &self,
        position: lsp_types::Position,
        auto_complete: bool,
    ) -> Option<Vec<lsp_types::CompletionItem>> {
        let position = Point {
            row: position.line as usize,
            column: position.character as usize,
        };

        let completions =
            code_completion::CodeCompletion::code_completion(self, position, auto_complete);
        completions.map(|mut completions| {
            completions
                .drain(..)
                .map(|item| lsp_types::CompletionItem {
                    label: item.label,
                    kind: Some(lsp_types::CompletionItemKind::from(item.kind)),
                    ..Default::default()
                })
                .collect()
        })
    }

    pub fn symbol_declaration(&self, position: lsp_types::Position) -> Option<String> {
        let position = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        let Some(context) = DocumentContext::context(self, position) else {
            return None;
        };

        if context.can_reference_variables()
            && let Some(symbol_name) = self.symbol_at_position(position)
            && let Some(variable) = self.find_local_variable(position, &symbol_name)
        {
            Some(variable.to_string())
        } else {
            let reference_resolver = ReferenceResolver::new(self);
            let symbols = reference_resolver.resolve_reference(context, position);
            symbols.map(|s| s.symbol.to_string()).next()
        }
    }

    pub fn signature_help(
        &self,
        position: lsp_types::Position,
    ) -> Option<Vec<lsp_types::SignatureInformation>> {
        let position = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        let parameter_info = parameter_info::ParameterInfo::parameter_info(self, position)?;
        Some(
            parameter_info
                .into_iter()
                .map(lsp_types::SignatureInformation::from)
                .collect(),
        )
    }

    pub fn document_highlight(
        &self,
        position: lsp_types::Position,
    ) -> Option<Vec<lsp_types::DocumentHighlight>> {
        let position = Point {
            row: position.line as usize,
            column: position.character as usize,
        };

        if let Some(range_pair) =
            scope_balancer::ScopeBalancer::open_and_close_scope_range_pair(self, position)
            && (std::ops::RangeInclusive::new(range_pair.0.start, range_pair.0.end)
                .contains(&position)
                || std::ops::RangeInclusive::new(range_pair.1.start, range_pair.1.end)
                    .contains(&position))
        {
            Some(
                [range_pair.0, range_pair.1]
                    .map(|r| lsp_types::DocumentHighlight {
                        range: lsp_types::Range::new(
                            lsp_types::Position::new(r.start.row as u32, r.start.column as u32),
                            lsp_types::Position::new(r.end.row as u32, r.end.column as u32),
                        ),
                        kind: Some(lsp_types::DocumentHighlightKind::TEXT),
                    })
                    .to_vec(),
            )
        } else {
            let allow_default_highlights = DocumentContext::context(self, position).is_some();
            if allow_default_highlights {
                None
            } else {
                Some(vec![])
            }
        }
    }

    pub fn document_symbols(&self) -> Vec<lsp_types::DocumentSymbol> {
        let Some(tree) = self.tree() else {
            return Vec::new();
        };
        index::IndexFile::with_parse_tree(tree, self.text_content().as_bytes())
            .symbols
            .iter()
            .map(|s| s.into())
            .collect()
    }

    pub fn code_lens_items(&self) -> Vec<lsp_types::CodeLens> {
        code_lens::CodeLens::code_lens(self)
            .into_iter()
            .map(|code_lens| lsp_types::CodeLens {
                range: lsp_types::Range::new(
                    lsp_types::Position::new(
                        code_lens.location.row as u32,
                        code_lens.location.column as u32,
                    ),
                    lsp_types::Position::new(code_lens.location.row as u32 + 1, 0),
                ),
                command: Some(lsp_types::Command {
                    title: code_lens.description,
                    ..Default::default()
                }),
                data: None,
            })
            .collect()
    }
}

impl From<code_completion::CompletionItemKind> for lsp_types::CompletionItemKind {
    fn from(kind: code_completion::CompletionItemKind) -> Self {
        match kind {
            code_completion::CompletionItemKind::Class => Self::CLASS,
            code_completion::CompletionItemKind::Method => Self::METHOD,
            code_completion::CompletionItemKind::Property => Self::PROPERTY,
            code_completion::CompletionItemKind::Object => Self::INTERFACE,
            code_completion::CompletionItemKind::LocalVariable => Self::VARIABLE,
            code_completion::CompletionItemKind::GlobalVariable => Self::VARIABLE,
            code_completion::CompletionItemKind::Function => Self::FUNCTION,
            code_completion::CompletionItemKind::StructMember => Self::FIELD,
            code_completion::CompletionItemKind::EnumMember => Self::ENUM_MEMBER,
            code_completion::CompletionItemKind::TableName => Self::FILE,
            code_completion::CompletionItemKind::TableColumn => Self::FIELD,
            code_completion::CompletionItemKind::Command => Self::KEYWORD,
        }
    }
}

impl From<&index::IndexSymbolSnapshot<'_, index::IndexSymbol>> for lsp_types::Location {
    fn from(symbol_snapshot: &index::IndexSymbolSnapshot<index::IndexSymbol>) -> Self {
        let location = symbol_snapshot.symbol.location();
        lsp_types::Location::new(
            lsp_types::Url::from_file_path(symbol_snapshot.path).unwrap(),
            lsp_types::Range::new(
                lsp_types::Position {
                    line: location.line as u32,
                    character: location.column as u32,
                },
                lsp_types::Position {
                    line: location.line as u32,
                    character: location.column as u32,
                },
            ),
        )
    }
}

impl From<parameter_info::ParameterInfo> for lsp_types::SignatureInformation {
    fn from(value: parameter_info::ParameterInfo) -> Self {
        Self {
            label: value.signature,
            documentation: None,
            parameters: Some(
                value
                    .parameters
                    .into_iter()
                    .map(|parameter| lsp_types::ParameterInformation {
                        label: lsp_types::ParameterLabel::Simple(parameter),
                        documentation: None,
                    })
                    .collect(),
            ),
            active_parameter: Some(value.active_parameter as u32),
        }
    }
}

impl From<&index::IndexSymbol> for lsp_types::DocumentSymbol {
    fn from(symbol: &index::IndexSymbol) -> Self {
        let position = lsp_types::Position {
            line: symbol.location().line as u32,
            character: symbol.location().column as u32,
        };

        let range = lsp_types::Range {
            start: lsp_types::Position {
                line: symbol.range().start.line as u32,
                character: symbol.range().start.column as u32,
            },
            end: lsp_types::Position {
                line: symbol.range().end.line as u32,
                character: symbol.range().end.column as u32,
            },
        };

        let children: Vec<lsp_types::DocumentSymbol> = symbol
            .children()
            .map(lsp_types::DocumentSymbol::from)
            .collect();

        #[allow(deprecated)]
        lsp_types::DocumentSymbol {
            name: symbol.name().to_string(),
            detail: Some(symbol.to_string()),
            kind: symbol.into(),
            tags: None,
            deprecated: None,
            range: range,
            selection_range: lsp_types::Range {
                start: position,
                end: position,
            },
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        }
    }
}

impl From<&index::IndexSymbol> for lsp_types::SymbolKind {
    fn from(symbol: &index::IndexSymbol) -> Self {
        match symbol {
            index::IndexSymbol::Class(_) => Self::CLASS,
            index::IndexSymbol::Object(_) => Self::OBJECT,
            index::IndexSymbol::Struct(_) => Self::STRUCT,
            index::IndexSymbol::Method(_) => Self::METHOD,
            index::IndexSymbol::Property(_) => Self::PROPERTY,
            index::IndexSymbol::Variable(_) => Self::VARIABLE,
            index::IndexSymbol::Alias(_) => Self::ENUM_MEMBER,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_content() {
        let mut doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        assert_eq!(
            doc.root_node().unwrap().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) superclass: (identifier)) (object_footer (keyword))))"
        );

        doc.replace_content(&"Procedure test\nEnd_Procedure\n".to_string());
        assert_eq!(
            doc.root_node().unwrap().to_sexp(),
            "(source_file (procedure_definition (procedure_header (keyword) name: (identifier)) (procedure_footer (keyword))))"
        );
    }

    #[test]
    fn test_edit_content() {
        let mut doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        assert_eq!(
            doc.root_node().unwrap().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) superclass: (identifier)) (object_footer (keyword))))"
        );

        doc.edit_content(&vec![lsp_types::TextDocumentContentChangeEvent {
            range: Some(tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 23,
                },
                end: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 23,
                },
            }),
            text: "\nProcedure test\nEnd_Procedure".to_string(),
            range_length: None,
        }]);

        assert_eq!(
            doc.root_node().unwrap().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) superclass: (identifier)) (procedure_definition (procedure_header (keyword) name: (identifier)) (procedure_footer (keyword))) (object_footer (keyword))))"
        );
    }

    #[test]
    fn test_local_variables() {
        let test_content = r#"
Object oMyObject is a cObject
    Procedure foo
        Integer iMyInt
        String sMyStr
        Move 1 to iMyInt
        Move "hello" to sMyStr
    End_Procedure

    Procedure bar Integer iArg1 String sArg2
        Integer iMyOtherInt iMyOtherIntOnSameLine
        Move 1 to iMyOtherInt
        Move i
    End_Procedure
End_Object

Send foo of oMyObject
            "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let mut variables = doc.local_variables(Point::new(5, 21));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 3, column: 16 }, range: SourceRange { start: SourceLocation { line: 3, column: 16 }, end: SourceLocation { line: 3, column: 22 } }, symbol_path: SymbolPath(\"iMyInt\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 4, column: 15 }, range: SourceRange { start: SourceLocation { line: 4, column: 15 }, end: SourceLocation { line: 4, column: 21 } }, symbol_path: SymbolPath(\"sMyStr\"), data_type: DataFlexDataType(\"String\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");

        let mut variables = doc.local_variables(Point::new(11, 23));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 26 }, range: SourceRange { start: SourceLocation { line: 9, column: 26 }, end: SourceLocation { line: 9, column: 31 } }, symbol_path: SymbolPath(\"iArg1\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 39 }, range: SourceRange { start: SourceLocation { line: 9, column: 39 }, end: SourceLocation { line: 9, column: 44 } }, symbol_path: SymbolPath(\"sArg2\"), data_type: DataFlexDataType(\"String\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 16 }, range: SourceRange { start: SourceLocation { line: 10, column: 16 }, end: SourceLocation { line: 10, column: 27 } }, symbol_path: SymbolPath(\"iMyOtherInt\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 28 }, range: SourceRange { start: SourceLocation { line: 10, column: 28 }, end: SourceLocation { line: 10, column: 49 } }, symbol_path: SymbolPath(\"iMyOtherIntOnSameLine\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");

        let mut variables = doc.local_variables(Point::new(12, 14));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 26 }, range: SourceRange { start: SourceLocation { line: 9, column: 26 }, end: SourceLocation { line: 9, column: 31 } }, symbol_path: SymbolPath(\"iArg1\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 39 }, range: SourceRange { start: SourceLocation { line: 9, column: 39 }, end: SourceLocation { line: 9, column: 44 } }, symbol_path: SymbolPath(\"sArg2\"), data_type: DataFlexDataType(\"String\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 16 }, range: SourceRange { start: SourceLocation { line: 10, column: 16 }, end: SourceLocation { line: 10, column: 27 } }, symbol_path: SymbolPath(\"iMyOtherInt\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 28 }, range: SourceRange { start: SourceLocation { line: 10, column: 28 }, end: SourceLocation { line: 10, column: 49 } }, symbol_path: SymbolPath(\"iMyOtherIntOnSameLine\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");
    }

    #[test]
    fn test_struct_local_variables() {
        let test_content = r#"
Struct tMyStruct
End_Struct

Procedure testIt
    tMyStruct myStructVar
    tNotExistingStruct myOtherStructVar
End_Procedure
            "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let mut variables = doc.local_variables(Point::new(5, 21));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 5, column: 14 }, range: SourceRange { start: SourceLocation { line: 5, column: 14 }, end: SourceLocation { line: 5, column: 25 } }, symbol_path: SymbolPath(\"myStructVar\"), data_type: DataFlexDataType(\"tMyStruct\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");
    }
}
