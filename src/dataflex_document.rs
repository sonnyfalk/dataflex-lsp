use tower_lsp::lsp_types::{SemanticToken, TextDocumentContentChangeEvent};
use tree_sitter::{InputEdit, Parser, Point, Tree};

mod line_map;
mod syntax_map;

pub struct DataFlexDocument {
    line_map: line_map::LineMap,
    parser: Parser,
    tree: Option<Tree>,
    syntax_map: Option<syntax_map::SyntaxMap>,
}

impl DataFlexDocument {
    pub fn new(text: &str) -> Self {
        let mut doc = Self {
            line_map: line_map::LineMap::new(text),
            parser: Self::make_parser(),
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
        self.tree = self.parser.parse_with(
            &mut |_, point| {
                self.line_map
                    .line_text_with_ending(point.row)
                    .and_then(|line| line.as_bytes().get(point.column..))
                    .unwrap_or(&[])
            },
            self.tree.as_ref(),
        );

        self.syntax_map = Some(syntax_map::SyntaxMap::new(self));
    }

    #[cfg(test)]
    pub fn replace_content(&mut self, text: &str) {
        self.line_map = line_map::LineMap::new(text);
        self.tree = None;
        self.update();
    }

    pub fn edit_content(&mut self, changes: &Vec<TextDocumentContentChangeEvent>) {
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

    pub fn semantic_tokens_full(&self) -> Option<Vec<SemanticToken>> {
        let syntax_map = self.syntax_map.as_ref()?;
        Some(syntax_map.get_all_tokens())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_content() {
        let mut doc = DataFlexDocument::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) (identifier)) (object_footer (keyword))))");

        doc.replace_content(&"Procedure test\nEnd_Procedure\n".to_string());
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (procedure_definition (procedure_header (keyword) name: (identifier)) (procedure_footer (keyword))))");
    }

    #[test]
    fn test_edit_content() {
        let mut doc = DataFlexDocument::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) (identifier)) (object_footer (keyword))))");

        doc.edit_content(&vec![TextDocumentContentChangeEvent {
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
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) (identifier)) (procedure_definition (procedure_header (keyword) name: (identifier)) (procedure_footer (keyword))) (object_footer (keyword))))");
    }
}
