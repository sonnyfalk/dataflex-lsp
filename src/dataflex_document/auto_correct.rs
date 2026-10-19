use super::*;
use index::Index;

pub struct AutoCorrect {}

impl AutoCorrect {
    pub fn is_auto_correct_trigger(text: &str) -> bool {
        matches!(text, " " | "\t" | "\r" | "\n" | "(" | ",")
    }

    pub fn auto_correct(
        doc: &DataFlexDocument,
        position: Point,
        index: &Index,
    ) -> Option<TextEdit> {
        let mut cursor = doc.cursor()?;
        if cursor.goto_leaf_node_at_or_before_point(position) && cursor.is_any_keyword() {
            let node = cursor.node();
            let kw = index::SymbolName::from(doc.line_map.text_for_node(&node));
            if let Some(command) = index.all_commands().find(|&command| *command == kw)
                && kw.to_string() != command.to_string()
            {
                return Some(TextEdit {
                    range: node.start_position()..node.end_position(),
                    text: command.to_string(),
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_correct_command() {
        let test_content = "if ";
        let index = index::Index::make_test_index();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, &index);
        let edits = AutoCorrect::auto_correct(&doc, Point::new(0, 2), &index);
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 0, column: 0 }..Point { row: 0, column: 2 }, text: \"If\" })"
        );
    }
}
