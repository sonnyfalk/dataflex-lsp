use std::ops::{Deref, DerefMut};

pub struct DataFlexTreeParser {
    parser: tree_sitter::Parser,
}

impl DataFlexTreeParser {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_dataflex::LANGUAGE.into())
            .expect("Error loading DataFlex grammar");
        Self { parser }
    }
}

impl Deref for DataFlexTreeParser {
    type Target = tree_sitter::Parser;

    fn deref(&self) -> &Self::Target {
        &self.parser
    }
}

impl DerefMut for DataFlexTreeParser {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parser
    }
}
