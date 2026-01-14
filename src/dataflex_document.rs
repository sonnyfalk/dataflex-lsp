use tower_lsp::lsp_types;
use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::index;

mod code_completion;
mod line_map;
mod syntax_map;

#[allow(dead_code)]
pub struct DataFlexDocument {
    line_map: line_map::LineMap,
    parser: Parser,
    index: index::IndexRef,
    tree: Option<Tree>,
    syntax_map: Option<syntax_map::SyntaxMap>,
}

impl DataFlexDocument {
    pub fn new(text: &str, index_ref: index::IndexRef) -> Self {
        let mut doc = Self {
            line_map: line_map::LineMap::new(text),
            parser: Self::make_parser(),
            index: index_ref,
            tree: None,
            syntax_map: None,
        };
        doc.update();
        doc
    }

    fn make_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_dataflex::LANGUAGE.into())
            .expect("Error loading DataFlex grammar");
        parser
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

    pub fn edit_content(&mut self, changes: &Vec<lsp_types::TextDocumentContentChangeEvent>) {
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
    }

    pub fn semantic_tokens_full(&self) -> Option<Vec<lsp_types::SemanticToken>> {
        let syntax_map = self.syntax_map.as_ref()?;
        Some(syntax_map.get_all_tokens())
    }

    pub fn find_definition(&self, position: lsp_types::Position) -> Option<lsp_types::Location> {
        let Some(tree) = self.tree.as_ref() else {
            return None;
        };
        let start = Point {
            row: position.line as usize,
            column: position.character as usize,
        };
        let Some(node) = tree.root_node().descendant_for_point_range(start, start) else {
            return None;
        };
        let name = self
            .line_map
            .text_in_range(node.start_position(), node.end_position());

        let index = self.index.get();
        let Some(class_symbol) = index.find_class(&index::SymbolName::from(name)) else {
            return None;
        };

        Some(lsp_types::Location::new(
            lsp_types::Url::from_file_path(class_symbol.path).unwrap(),
            lsp_types::Range::new(
                lsp_types::Position {
                    line: class_symbol.symbol.location.row as u32,
                    character: class_symbol.symbol.location.column as u32,
                },
                lsp_types::Position {
                    line: class_symbol.symbol.location.row as u32,
                    character: class_symbol.symbol.location.column as u32,
                },
            ),
        ))
    }

    pub fn code_completion(
        &self,
        position: lsp_types::Position,
    ) -> Option<Vec<lsp_types::CompletionItem>> {
        let position = Point {
            row: position.line as usize,
            column: position.character as usize,
        };

        let completions = code_completion::CodeCompletion::code_completion(self, position);
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
}

impl From<code_completion::CompletionItemKind> for lsp_types::CompletionItemKind {
    fn from(kind: code_completion::CompletionItemKind) -> Self {
        match kind {
            code_completion::CompletionItemKind::Class => Self::CLASS,
            code_completion::CompletionItemKind::Method => Self::METHOD,
            code_completion::CompletionItemKind::Property => Self::PROPERTY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_content() {
        let mut doc = DataFlexDocument::new(
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) superclass: (identifier)) (object_footer (keyword))))");

        doc.replace_content(&"Procedure test\nEnd_Procedure\n".to_string());
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (procedure_definition (procedure_header (keyword) name: (identifier)) (procedure_footer (keyword))))");
    }

    #[test]
    fn test_edit_content() {
        let mut doc = DataFlexDocument::new(
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) superclass: (identifier)) (object_footer (keyword))))");

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

        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) superclass: (identifier)) (procedure_definition (procedure_header (keyword) name: (identifier)) (procedure_footer (keyword))) (object_footer (keyword))))");
    }
}
