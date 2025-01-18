use tower_lsp::lsp_types::{SemanticToken, TextDocumentContentChangeEvent};
use tree_sitter::{Parser, Tree};

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
            None,
        );

        self.syntax_map = Some(syntax_map::SyntaxMap::new(self));
    }

    pub fn replace_content(&mut self, text: &str) {
        self.line_map = line_map::LineMap::new(text);
        self.update();
    }

    pub fn edit_content(&mut self, changes: &Vec<TextDocumentContentChangeEvent>) {
        for change in changes {
            let Some(range) = change.range else {
                self.replace_content(&change.text);
                continue;
            };
            // TODO: Convert UTF-16 to UTF-8 range.
        }
    }

    pub fn semantic_tokens_full(&self) -> Option<Vec<SemanticToken>> {
        let syntax_map = self.syntax_map.as_ref()?;
        Some(syntax_map.get_tokens())
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
}
