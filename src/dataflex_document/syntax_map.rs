use streaming_iterator::StreamingIterator;
use tower_lsp::lsp_types::SemanticToken;
use tree_sitter::{Point, Query, QueryCursor};

use super::*;

pub struct SyntaxMap {
    tokens: Vec<SemanticToken>,
}

impl SyntaxMap {
    pub fn new(doc: &DataFlexDocument) -> Self {
        let tokens = Self::semantic_tokens(doc);

        Self { tokens }
    }

    pub fn get_tokens(&self) -> Vec<SemanticToken> {
        self.tokens.clone()
    }

    fn semantic_tokens(doc: &DataFlexDocument) -> Vec<SemanticToken> {
        let query = Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            tree_sitter_dataflex::HIGHLIGHTS_QUERY,
        )
        .expect("Error loading HIGHLIGHTS_QUERY");

        let tree = doc.tree.as_ref().unwrap();
        let mut query_cursor = QueryCursor::new();
        let mut captures =
            query_cursor.captures(&query, tree.root_node(), doc.line_map.text_provider());
        let capture_names = query.capture_names();

        let mut prev_pos = Point { row: 0, column: 0 };
        let mut tokens = Vec::new();
        while let Some(query_match) = captures.next() {
            for capture in query_match.0.captures {
                let pos = capture.node.start_position();
                let token = match capture_names[capture.index as usize] {
                    "keyword" => Some(SemanticToken {
                        delta_line: (pos.row - prev_pos.row) as u32,
                        delta_start: if pos.row == prev_pos.row {
                            (pos.column - prev_pos.column) as u32
                        } else {
                            (pos.column) as u32
                        },
                        length: (capture.node.end_position().column - pos.column) as u32,
                        token_type: 0,
                        token_modifiers_bitset: 0,
                    }),
                    _ => None,
                };
                if let Some(token) = token {
                    tokens.push(token);
                    prev_pos = pos;
                }
            }
        }

        tokens
    }
}
