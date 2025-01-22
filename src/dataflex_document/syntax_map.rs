use streaming_iterator::StreamingIterator;
use tower_lsp::lsp_types::SemanticToken;
use tree_sitter::{Point, Query, QueryCursor};

use super::*;

pub struct SyntaxMap {
    tokens: Vec<SyntaxToken>,
}

struct SyntaxToken {
    position: Point,
    length: u32,
    kind: u32,
}

impl SyntaxMap {
    pub fn new(doc: &DataFlexDocument) -> Self {
        let tokens = Self::generate_tokens(doc);

        Self { tokens }
    }

    pub fn get_tokens(&self) -> Vec<SemanticToken> {
        let mut prev_pos = Point { row: 0, column: 0 };
        self.tokens
            .iter()
            .map(|token| {
                let sem_token = SemanticToken {
                    delta_line: (token.position.row - prev_pos.row) as u32,
                    delta_start: if token.position.row == prev_pos.row {
                        (token.position.column - prev_pos.column) as u32
                    } else {
                        (token.position.column) as u32
                    },
                    length: token.length,
                    token_type: token.kind,
                    token_modifiers_bitset: 0,
                };
                prev_pos = token.position;
                sem_token
            })
            .collect()
    }

    fn generate_tokens(doc: &DataFlexDocument) -> Vec<SyntaxToken> {
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

        let mut tokens = Vec::new();
        while let Some(query_match) = captures.next() {
            for capture in query_match.0.captures {
                let start = capture.node.start_position();
                let end = capture.node.end_position();
                if start.row != end.row {
                    //FIXME: Break up multi-line tokens
                    continue;
                }
                let len = end.column - start.column;
                let token = match capture_names[capture.index as usize] {
                    "keyword" => Some(SyntaxToken {
                        position: start,
                        length: len as u32,
                        kind: 0,
                    }),
                    _ => None,
                };
                if let Some(token) = token {
                    tokens.push(token);
                }
            }
        }

        tokens
    }
}
