#[derive(Debug)]
pub struct TextEdit {
    pub range: std::ops::Range<tree_sitter::Point>,
    pub text: String,
}
