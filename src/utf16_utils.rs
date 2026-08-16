pub trait StrExt {
    fn utf16_count(&self) -> usize;
    fn utf8_index_from_utf16_index(&self, index: usize) -> usize;
    fn utf16_index_from_utf8_index(&self, index: usize) -> usize;
}

impl StrExt for str {
    fn utf16_count(&self) -> usize {
        if self.is_ascii() {
            self.len()
        } else {
            self.chars().map(|c| c.len_utf16()).sum()
        }
    }

    fn utf8_index_from_utf16_index(&self, index: usize) -> usize {
        if self.is_ascii() {
            index
        } else {
            self.chars()
                .scan((0, 0), |(utf8_index, utf16_index), c| {
                    *utf8_index += c.len_utf8();
                    *utf16_index += c.len_utf16();
                    Some((*utf8_index, *utf16_index))
                })
                .take_while(|(_, utf16_index)| *utf16_index <= index)
                .map(|(utf8_index, _)| utf8_index)
                .last()
                .unwrap_or_default()
        }
    }

    fn utf16_index_from_utf8_index(&self, index: usize) -> usize {
        if self.is_ascii() {
            index
        } else {
            self.chars()
                .scan((0, 0), |(utf8_index, utf16_index), c| {
                    *utf8_index += c.len_utf8();
                    *utf16_index += c.len_utf16();
                    Some((*utf8_index, *utf16_index))
                })
                .take_while(|(utf8_index, _)| *utf8_index <= index)
                .map(|(_, utf16_index)| utf16_index)
                .last()
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf16_count() {
        assert_eq!("hello".utf16_count(), 5);
        assert_eq!("åäö".len(), 6);
        assert_eq!("åäö".utf16_count(), 3);
    }

    #[test]
    fn test_utf8_index_from_utf16_index() {
        assert_eq!("hello".utf8_index_from_utf16_index(0), 0);
        assert_eq!("hello".utf8_index_from_utf16_index(1), 1);
        assert_eq!("hello".utf8_index_from_utf16_index(5), 5);
        assert_eq!("åäö".utf8_index_from_utf16_index(0), 0);
        assert_eq!("åäö".utf8_index_from_utf16_index(1), 2);
        assert_eq!("åäö".utf8_index_from_utf16_index(2), 4);
        assert_eq!("åäö".utf8_index_from_utf16_index(3), 6);
    }

    #[test]
    fn test_utf16_index_from_utf8_index() {
        assert_eq!("hello".utf16_index_from_utf8_index(0), 0);
        assert_eq!("hello".utf16_index_from_utf8_index(1), 1);
        assert_eq!("hello".utf16_index_from_utf8_index(5), 5);
        assert_eq!("åäö".utf16_index_from_utf8_index(0), 0);
        assert_eq!("åäö".utf16_index_from_utf8_index(2), 1);
        assert_eq!("åäö".utf16_index_from_utf8_index(4), 2);
        assert_eq!("åäö".utf16_index_from_utf8_index(6), 3);
    }
}
