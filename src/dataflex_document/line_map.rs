use line_span::LineSpanExt;
use tree_sitter::{Node, Point};

pub struct LineMap {
    lines: Vec<String>,
}

impl LineMap {
    pub fn new(text: &String) -> Self {
        let lines = text
            .line_spans()
            .map(|l| l.as_str_with_ending().to_string())
            .collect();
        Self { lines }
    }

    pub fn line_text_with_ending(&self, line: usize) -> Option<&str> {
        self.lines.get(line).and_then(|l| Some(l.as_str()))
    }

    #[allow(dead_code)]
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
        let line_map = LineMap::new(&"Object oTest is a cTest\nEnd_Object\n".to_string());
        assert_eq!(line_map.lines.len(), 2);

        assert_eq!(
            line_map.line_text_with_ending(0),
            Some("Object oTest is a cTest\n")
        );
        assert_eq!(line_map.line_text_with_ending(1), Some("End_Object\n"));
        assert_eq!(line_map.line_text_with_ending(2), None);
    }

    #[test]
    fn test_text_in_range() {
        let line_map = LineMap::new(&"Object oTest is a cTest\nEnd_Object\n".to_string());
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
}
