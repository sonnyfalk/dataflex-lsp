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
