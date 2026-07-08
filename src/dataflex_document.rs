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
mod symbol_declaration;
mod syntax_map;
mod tree_cursor;

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

        let reference_resolver = ReferenceResolver::new(self);
        let locations = if context.can_reference_variables()
            && let Some(variable) = reference_resolver.resolve_local_variable(position)
        {
            vec![lsp_types::Location::new(
                lsp_types::Url::from_file_path(&self.file_path).unwrap(),
                lsp_types::Range::from(index::SourceRange::with_location(variable.location)),
            )]
        } else if context.can_reference_tables()
            && let Some(table_ref) = reference_resolver.resolve_table_reference(position)
        {
            vec![lsp_types::Location::new(
                lsp_types::Url::from_file_path(&table_ref.file.path).unwrap(),
                lsp_types::Range::from(index::SourceRange::with_location(index::SourceLocation {
                    line: 0,
                    column: 0,
                })),
            )]
        } else if context.is_file_reference()
            && let Some(file_ref) = self.node_at_position(position).map(|node| {
                index::IndexFileRef::from(&PathBuf::from(self.line_map.text_for_node(&node)))
            })
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
            let symbols = reference_resolver.resolve_reference(context, position);
            symbols
                .map(|qualified_symbol| lsp_types::Location::from(&qualified_symbol))
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
                    label_details: item.details.map(|details| {
                        lsp_types::CompletionItemLabelDetails {
                            detail: Some(details),
                            description: None,
                        }
                    }),
                    insert_text: item.insert_text,
                    ..Default::default()
                })
                .collect()
        })
    }

    pub fn symbol_declaration(
        &self,
        position: lsp_types::Position,
    ) -> Option<lsp_types::MarkedString> {
        let position = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        let context = DocumentContext::context(self, position)?;

        let reference_resolver = ReferenceResolver::new(self);
        if context.can_reference_variables()
            && let Some(variable) = reference_resolver.resolve_local_variable(position)
        {
            Some(lsp_types::MarkedString::from_language_code(
                "dataflex".into(),
                variable.to_string(),
            ))
        } else if context.can_reference_tables()
            && let Some(table_ref) = reference_resolver.resolve_table_reference(position)
        {
            Some(lsp_types::MarkedString::String(format!(
                "Table: {}",
                table_ref.table.name
            )))
        } else {
            let symbols = reference_resolver.resolve_reference(context, position);
            symbols
                .map(|s| symbol_declaration::SymbolDeclaration::new(&s, &self.index.get()))
                .map(|symbol_declaration| {
                    lsp_types::MarkedString::from_markdown(symbol_declaration.to_string())
                })
                .next()
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
            code_completion::CompletionItemKind::Text => Self::TEXT,
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
            code_completion::CompletionItemKind::File => Self::FILE,
            code_completion::CompletionItemKind::Struct => Self::STRUCT,
        }
    }
}

impl From<&index::QualifiedIndexSymbol<'_>> for lsp_types::Location {
    fn from(qualified_symbol: &index::QualifiedIndexSymbol) -> Self {
        let location = qualified_symbol.symbol.location();
        lsp_types::Location::new(
            lsp_types::Url::from_file_path(&qualified_symbol.file.path).unwrap(),
            lsp_types::Range::from(index::SourceRange::with_location(location)),
        )
    }
}

impl From<index::SourceRange> for lsp_types::Range {
    fn from(range: index::SourceRange) -> Self {
        lsp_types::Range::new(
            lsp_types::Position {
                line: range.start.line as u32,
                character: range.start.column as u32,
            },
            lsp_types::Position {
                line: range.end.line as u32,
                character: range.end.column as u32,
            },
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

        doc.replace_content("Procedure test\nEnd_Procedure\n");
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
}
