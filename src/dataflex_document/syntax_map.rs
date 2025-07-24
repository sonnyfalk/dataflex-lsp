use std::ops::{Bound, RangeBounds};
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

    pub fn get_all_tokens(&self) -> Vec<SemanticToken> {
        self.get_tokens_for_lines(0..self.lines.len())
    }

    pub fn get_tokens_for_lines(&self, line_range: impl RangeBounds<usize>) -> Vec<SemanticToken> {
        let line_range = match line_range.start_bound() {
            Bound::Included(start) => *start,
            Bound::Excluded(start) => start + 1,
            Bound::Unbounded => 0,
        }..match line_range.end_bound() {
            Bound::Included(end) => end + 1,
            Bound::Excluded(end) => *end,
            Bound::Unbounded => self.lines.len(),
        };

        let prev_row = if line_range.start > 0 {
            (0..line_range.start)
                .rev()
                .find(|i| !self.lines[*i].tokens.is_empty())
                .unwrap_or(0)
        } else {
            0
        };
        let row_offset = line_range.start;
        let (sem_tokens, _) = self.lines[line_range].iter().enumerate().fold(
            (Vec::new(), prev_row),
            |(sem_tokens, prev_row), (relative_row, line)| {
                line.tokens.iter().fold(
                    (sem_tokens, prev_row),
                    |(mut sem_tokens, prev_row), token| {
                        let row = row_offset + relative_row;
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
                                "entity.other.inherited-class" => {
                                    let name = doc.line_map.text_in_range(start, end);
                                    if doc
                                        .index
                                        .get()
                                        .is_known_class(&index::SymbolName::from(name))
                                    {
                                        Some(SyntaxToken {
                                            delta_start: if start.row == prev_pos.row {
                                                (start.column - prev_pos.column) as u32
                                            } else {
                                                start.column as u32
                                            },
                                            length: (end.column - start.column) as u32,
                                            kind: 1,
                                        })
                                    } else {
                                        None
                                    }
                                }
                                "entity.name.function" => {
                                    let name = doc.line_map.text_in_range(start, end);
                                    if doc
                                        .index
                                        .get()
                                        .is_known_method(&index::SymbolName::from(name))
                                    {
                                        Some(SyntaxToken {
                                            delta_start: if start.row == prev_pos.row {
                                                (start.column - prev_pos.column) as u32
                                            } else {
                                                start.column as u32
                                            },
                                            length: (end.column - start.column) as u32,
                                            kind: 2,
                                        })
                                    } else {
                                        None
                                    }
                                }
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
        let doc = DataFlexDocument::new(
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
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
                },
                Line { tokens: vec![] }
            ]
        );
    }

    #[test]
    fn test_get_all_tokens() {
        let doc = DataFlexDocument::new(
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let tokens = doc.syntax_map.unwrap().get_all_tokens();
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

    #[test]
    fn test_get_tokens_for_lines() {
        let doc = DataFlexDocument::new(
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let syntax_map = doc.syntax_map.as_ref().unwrap();
        assert_eq!(
            syntax_map.get_tokens_for_lines(0..1),
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
            ]
        );

        assert_eq!(
            syntax_map.get_tokens_for_lines(1..2),
            [SemanticToken {
                delta_line: 1,
                delta_start: 0,
                length: 10,
                token_type: 0,
                token_modifiers_bitset: 0
            }]
        );
    }
}
