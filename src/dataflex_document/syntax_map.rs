use streaming_iterator::StreamingIterator;
use tower_lsp::lsp_types::SemanticToken;
use tree_sitter::{Point, Query, QueryCursor};

use super::*;

pub struct SyntaxMap {
    lines: Vec<Line>,
}

#[derive(Debug, Eq, PartialEq)]
struct Line {
    tokens: Vec<SyntaxToken>,
}

#[derive(Debug, Eq, PartialEq)]
struct SyntaxToken {
    delta_start: u32,
    length: u32,
    kind: u32,
}

impl SyntaxMap {
    pub fn new(doc: &DataFlexDocument) -> Self {
        let lines = Self::generate_lines(doc);

        Self { lines }
    }

    pub fn get_tokens(&self) -> Vec<SemanticToken> {
        let (sem_tokens, _) = self.lines.iter().enumerate().fold(
            (Vec::new(), 0),
            |(sem_tokens, prev_row), (row, line)| {
                line.tokens.iter().fold(
                    (sem_tokens, prev_row),
                    |(mut sem_tokens, prev_row), token| {
                        sem_tokens.push(SemanticToken {
                            delta_line: (row - prev_row) as u32,
                            delta_start: token.delta_start,
                            length: token.length,
                            token_type: token.kind,
                            token_modifiers_bitset: 0,
                        });
                        (sem_tokens, row)
                    },
                )
            },
        );
        sem_tokens
    }

    fn generate_lines(doc: &DataFlexDocument) -> Vec<Line> {
        let query = Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            tree_sitter_dataflex::HIGHLIGHTS_QUERY,
        )
        .expect("Error loading HIGHLIGHTS_QUERY");

        let tree = doc.tree.as_ref().unwrap();
        let mut query_cursor = QueryCursor::new();
        let captures =
            query_cursor.captures(&query, tree.root_node(), doc.line_map.text_provider());
        let capture_names = query.capture_names();

        let mut lines = Vec::with_capacity(doc.line_map.line_count());
        lines.resize_with(doc.line_map.line_count(), || Line { tokens: Vec::new() });

        let (lines, _) = captures.fold(
            (lines, Point { row: 0, column: 0 }),
            |(lines, prev_pos), query_match| {
                query_match.0.captures.iter().fold(
                    (lines, prev_pos),
                    |(mut lines, prev_pos), capture| {
                        let start = capture.node.start_position();
                        let end = capture.node.end_position();
                        if start.row == end.row {
                            let token = match capture_names[capture.index as usize] {
                                "keyword" => Some(SyntaxToken {
                                    delta_start: if start.row == prev_pos.row {
                                        (start.column - prev_pos.column) as u32
                                    } else {
                                        start.column as u32
                                    },
                                    length: (end.column - start.column) as u32,
                                    kind: 0,
                                }),
                                _ => None,
                            };
                            if let Some(token) = token {
                                lines[start.row].tokens.push(token);
                                (lines, start)
                            } else {
                                (lines, prev_pos)
                            }
                        } else {
                            //FIXME: Break up multi-line tokens
                            (lines, prev_pos)
                        }
                    },
                )
            },
        );

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines() {
        let doc = DataFlexDocument::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(
            doc.syntax_map.unwrap().lines,
            [
                Line {
                    tokens: vec![
                        SyntaxToken {
                            delta_start: 0,
                            length: 6,
                            kind: 0
                        },
                        SyntaxToken {
                            delta_start: 13,
                            length: 2,
                            kind: 0
                        },
                        SyntaxToken {
                            delta_start: 3,
                            length: 1,
                            kind: 0
                        }
                    ]
                },
                Line {
                    tokens: vec![SyntaxToken {
                        delta_start: 0,
                        length: 10,
                        kind: 0
                    }]
                }
            ]
        );
    }

    #[test]
    fn test_get_tokens() {
        let doc = DataFlexDocument::new("Object oTest is a cTest\nEnd_Object\n");
        let tokens = doc.syntax_map.unwrap().get_tokens();
        assert_eq!(
            tokens,
            [
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 6,
                    token_type: 0,
                    token_modifiers_bitset: 0
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 13,
                    length: 2,
                    token_type: 0,
                    token_modifiers_bitset: 0
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 3,
                    length: 1,
                    token_type: 0,
                    token_modifiers_bitset: 0
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 10,
                    token_type: 0,
                    token_modifiers_bitset: 0
                }
            ]
        );
    }
}
