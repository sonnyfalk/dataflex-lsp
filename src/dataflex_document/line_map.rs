use line_span::{LineSpan, LineSpanExt, LineSpanIter};
use std::ops::{Bound, RangeBounds};
use tree_sitter::{Node, Point};

pub struct LineMap {
    lines: Vec<Line>,
}

struct Line {
    text: String,
}

impl LineMap {
    pub fn new(text: &str) -> Self {
        let lines = text
            .line_spans()
            .map(|l| Line {
                text: l.as_str_with_ending().to_string(),
            })
            .collect();
        Self { lines }
    }

    pub fn line_text_with_ending(&self, line: usize) -> Option<&str> {
        self.lines.get(line).and_then(|l| Some(l.text.as_str()))
    }

    #[cfg(test)]
    pub fn text_in_range(&self, start: Point, end: Point) -> String {
        self.text_in_range_iterator(start, end)
            .fold(String::new(), |text, s| text + s)
    }

    pub fn text_in_range_iterator<'a>(
        &'a self,
        start: Point,
        end: Point,
    ) -> TextInRangeIterator<'a> {
        TextInRangeIterator::new(self, start, end)
    }

    pub fn text_provider<'a>(&'a self) -> impl tree_sitter::TextProvider<&'a [u8]> {
        |node: Node| {
            self.text_in_range_iterator(node.start_position(), node.end_position())
                .map(|t| t.as_bytes())
        }
    }

    pub fn replace_range(&mut self, start: Point, end: Point, text: &str) {
        if start.row == end.row {
            let mut text_it = text.line_spans();
            if let Some(line_span) = text_it.next() {
                let tail =
                    self.lines[start.row].replace_range(start.column..end.column, Some(&line_span));
                let line = start.row + 1;
                self.splice_lines(line..line, text_it, tail);
            } else {
                self.lines[start.row].replace_range(start.column..end.column, None);
            }
        } else {
            let mut text_it = text.line_spans();
            let tail = self.lines[start.row].replace_range(start.column.., text_it.next().as_ref());
            self.lines[end.row].replace_range(..end.column, None);
            self.splice_lines((start.row + 1)..(end.row), text_it, tail);
            self.fixup_line_ending_if_needed(start.row);
        }
    }

    fn splice_lines(
        &mut self,
        line_range: std::ops::Range<usize>,
        it: LineSpanIter,
        tail: Option<String>,
    ) {
        let mut current_line = line_range.start;
        let mut inserted_lines = 0;
        self.lines.splice(
            line_range,
            it.map(|l| {
                inserted_lines += 1;
                Line {
                    text: l.as_str_with_ending().to_string(),
                }
            }),
        );

        if inserted_lines > 0 {
            current_line += inserted_lines - 1;
        }

        if !self.lines[current_line].has_line_ending() {
            if let Some(tail) = tail {
                self.lines[current_line].text.push_str(&tail);
            } else {
                self.fixup_line_ending_if_needed(current_line);
            }
        } else if let Some(tail) = tail {
            self.lines.insert(current_line, Line { text: tail });
        }
    }

    fn fixup_line_ending_if_needed(&mut self, line: usize) {
        if !self.lines[line].has_line_ending() && (line + 1) < self.line_count() {
            let next_line = self.lines.remove(line + 1);
            self.lines[line].text.push_str(&next_line.text);
        }
    }

    #[cfg(test)]
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .fold(String::new(), |text, l| text + &l.text)
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn offset_at_point(&self, point: Point) -> usize {
        self.lines[..point.row]
            .iter()
            .fold(0, |offset, l| offset + l.text.len())
            + point.column
    }

    pub fn point_at_offset(&self, offset: usize) -> Point {
        let mut byte_index = 0;
        for (line_index, line) in self.lines.iter().enumerate() {
            let line_len = line.text.len();
            if byte_index + line_len > offset {
                return Point {
                    row: line_index,
                    column: offset - byte_index,
                };
            }
            byte_index += line_len;
        }
        Point {
            row: self.lines.len(),
            column: 0,
        }
    }
}

impl Line {
    fn replace_range(
        &mut self,
        range: impl RangeBounds<usize>,
        line_span: Option<&LineSpan>,
    ) -> Option<String> {
        let Some(line_span) = line_span else {
            self.text.replace_range(range, "");
            return None;
        };

        if line_span.ending_str().is_empty() {
            self.text.replace_range(range, line_span.as_str());
            None
        } else {
            let tail = match range.end_bound() {
                Bound::Included(offset) => Some(self.text.split_off(offset + 1)),
                Bound::Excluded(offset) => Some(self.text.split_off(*offset)),
                Bound::Unbounded => None,
            };
            self.text
                .replace_range(range, line_span.as_str_with_ending());
            tail.filter(|t| !t.is_empty())
        }
    }

    fn has_line_ending(&self) -> bool {
        self.text.ends_with("\n")
    }
}

pub struct TextInRangeIterator<'a> {
    line_map: &'a LineMap,
    start: Point,
    end: Point,
    next_line: Option<usize>,
}

impl<'a> TextInRangeIterator<'a> {
    fn new(line_map: &'a LineMap, start: Point, end: Point) -> Self {
        Self {
            line_map,
            start,
            end,
            next_line: Some(start.row),
        }
    }
}

impl<'a> Iterator for TextInRangeIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(current_line) = self.next_line else {
            return None;
        };

        if self.start.row == self.end.row {
            self.next_line = None;
            Some(
                &self
                    .line_map
                    .line_text_with_ending(self.start.row)
                    .unwrap_or("")[self.start.column..self.end.column],
            )
        } else if current_line == self.start.row {
            self.next_line = Some(current_line + 1);
            Some(
                &self
                    .line_map
                    .line_text_with_ending(current_line)
                    .unwrap_or("")[self.start.column..],
            )
        } else if current_line == self.end.row {
            self.next_line = None;
            Some(
                &self
                    .line_map
                    .line_text_with_ending(current_line)
                    .unwrap_or("")[..self.end.column],
            )
        } else {
            self.next_line = Some(current_line + 1);
            Some(
                &self
                    .line_map
                    .line_text_with_ending(current_line)
                    .unwrap_or(""),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_text_with_ending() {
        let line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.line_count(), 2);

        assert_eq!(
            line_map.line_text_with_ending(0),
            Some("Object oTest is a cTest\n")
        );
        assert_eq!(line_map.line_text_with_ending(1), Some("End_Object\n"));
        assert_eq!(line_map.line_text_with_ending(2), None);
    }

    #[test]
    fn test_text_in_range() {
        let line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(
            line_map.text_in_range(
                Point { row: 0, column: 0 },
                Point {
                    row: 0,
                    column: (6)
                }
            ),
            "Object"
        );

        assert_eq!(
            line_map.text_in_range(
                Point { row: 0, column: 0 },
                Point {
                    row: 1,
                    column: (10)
                }
            ),
            "Object oTest is a cTest\nEnd_Object"
        );

        assert_eq!(
            line_map.text_in_range(
                Point { row: 0, column: 0 },
                Point {
                    row: 2,
                    column: (0)
                }
            ),
            "Object oTest is a cTest\nEnd_Object\n"
        );
    }

    #[test]
    fn test_insert_text() {
        let mut line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.text(), "Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.line_count(), 2);

        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object oTest is a cTest\n"
        );
        assert_eq!(line_map.text(), "Object oTest is a cTest\nEnd_Object\n");

        line_map.replace_range(
            Point { row: 0, column: 12 },
            Point { row: 0, column: 12 },
            "It",
        );
        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object oTestIt is a cTest\n"
        );

        assert_eq!(line_map.text(), "Object oTestIt is a cTest\nEnd_Object\n");
        assert_eq!(line_map.line_count(), 2);
    }

    #[test]
    fn test_insert_new_line() {
        let mut line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.text(), "Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.line_count(), 2);

        line_map.replace_range(
            Point { row: 0, column: 23 },
            Point { row: 0, column: 23 },
            "\n",
        );

        assert_eq!(line_map.text(), "Object oTest is a cTest\n\nEnd_Object\n");
        assert_eq!(line_map.line_count(), 3);
        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object oTest is a cTest\n"
        );
        assert_eq!(line_map.line_text_with_ending(1).unwrap(), "\n");
        assert_eq!(line_map.line_text_with_ending(2).unwrap(), "End_Object\n");
    }

    #[test]
    fn test_insert_multiline_text() {
        let mut line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");

        assert_eq!(line_map.text(), "Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.line_count(), 2);

        line_map.replace_range(
            Point { row: 0, column: 23 },
            Point { row: 0, column: 23 },
            "\n    Procedure foo\n    End_Procedure",
        );

        assert_eq!(
            line_map.text(),
            "Object oTest is a cTest\n    Procedure foo\n    End_Procedure\nEnd_Object\n"
        );

        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object oTest is a cTest\n"
        );
        assert_eq!(
            line_map.line_text_with_ending(1).unwrap(),
            "    Procedure foo\n"
        );
        assert_eq!(
            line_map.line_text_with_ending(2).unwrap(),
            "    End_Procedure\n"
        );

        assert_eq!(line_map.line_count(), 4);
    }

    #[test]
    fn test_delete_text() {
        let mut line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        line_map.replace_range(
            Point { row: 0, column: 8 },
            Point { row: 0, column: 12 },
            "",
        );
        assert_eq!(line_map.text(), "Object o is a cTest\nEnd_Object\n");
        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object o is a cTest\n"
        );
        assert_eq!(line_map.line_text_with_ending(1).unwrap(), "End_Object\n");
        assert_eq!(line_map.line_count(), 2);
    }

    #[test]
    fn test_delete_text_across_lines() {
        let mut line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        line_map.replace_range(
            Point { row: 0, column: 18 },
            Point { row: 1, column: 3 },
            "",
        );
        assert_eq!(line_map.text(), "Object oTest is a _Object\n");
        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object oTest is a _Object\n"
        );

        assert_eq!(line_map.line_count(), 1);
    }

    #[test]
    fn test_replace_text() {
        let mut line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        line_map.replace_range(
            Point { row: 0, column: 8 },
            Point { row: 0, column: 12 },
            "MyTest",
        );
        assert_eq!(line_map.text(), "Object oMyTest is a cTest\nEnd_Object\n");
        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object oMyTest is a cTest\n"
        );
        assert_eq!(line_map.line_text_with_ending(1).unwrap(), "End_Object\n");
        assert_eq!(line_map.line_count(), 2);
    }

    #[test]
    fn test_replace_text_across_lines() {
        let mut line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        line_map.replace_range(
            Point { row: 0, column: 18 },
            Point { row: 1, column: 3 },
            "cMyTest\n",
        );
        assert_eq!(line_map.text(), "Object oTest is a cMyTest\n_Object\n");
        assert_eq!(
            line_map.line_text_with_ending(0).unwrap(),
            "Object oTest is a cMyTest\n"
        );
        assert_eq!(line_map.line_text_with_ending(1).unwrap(), "_Object\n");

        assert_eq!(line_map.line_count(), 2);
    }

    #[test]
    fn test_offset_at_point() {
        let line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.offset_at_point(Point { row: 0, column: 0 }), 0);
        assert_eq!(line_map.offset_at_point(Point { row: 0, column: 1 }), 1);
        assert_eq!(line_map.offset_at_point(Point { row: 0, column: 23 }), 23);
        assert_eq!(line_map.offset_at_point(Point { row: 1, column: 0 }), 24);
        assert_eq!(line_map.offset_at_point(Point { row: 1, column: 4 }), 28);
        assert_eq!(line_map.offset_at_point(Point { row: 2, column: 0 }), 35);
    }

    #[test]
    fn test_point_at_offset() {
        let line_map = LineMap::new("Object oTest is a cTest\nEnd_Object\n");
        assert_eq!(line_map.point_at_offset(0), Point { row: 0, column: 0 });
        assert_eq!(line_map.point_at_offset(1), Point { row: 0, column: 1 });
        assert_eq!(line_map.point_at_offset(23), Point { row: 0, column: 23 });
        assert_eq!(line_map.point_at_offset(24), Point { row: 1, column: 0 });
        assert_eq!(line_map.point_at_offset(28), Point { row: 1, column: 4 });
        assert_eq!(line_map.point_at_offset(35), Point { row: 2, column: 0 });
    }
}
