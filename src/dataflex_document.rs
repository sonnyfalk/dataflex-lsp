use tree_sitter::{Parser, Tree};

pub struct DataFlexDocument {
    text: String,
    parser: Parser,
    tree: Option<Tree>,
}

impl DataFlexDocument {
    pub fn new(text: String) -> Self {
        let mut doc = Self {
            text,
            parser: Self::make_parser(),
            tree: None,
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
        self.tree = self.parser.parse(self.text.as_bytes(), None);
    }

    pub fn replace_content(&mut self, text: String) {
        self.text = text;

        self.update();
    }
}

#[cfg(test)]
mod tests {
    use super::DataFlexDocument;

    #[test]
    fn test_replace_content() {
        let mut doc = DataFlexDocument::new("Object oTest is a cTest\nEnd_Object\n".to_string());
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (object_definition (object_header (keyword) name: (identifier) (keyword) (keyword) (identifier)) (object_footer (keyword))))");

        doc.replace_content("Procedure test\nEnd_Procedure\n".to_string());
        assert_eq!(doc.tree.as_ref().unwrap().root_node().to_sexp(),
            "(source_file (procedure_definition (procedure_header (keyword) name: (identifier)) (procedure_footer (keyword))))");
    }
}
