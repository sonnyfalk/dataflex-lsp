pub trait StrExt {
    fn utf16_count(&self) -> usize;
}

impl StrExt for str {
    fn utf16_count(&self) -> usize {
        if self.is_ascii() {
            self.len()
        } else {
            self.chars().map(|c| c.len_utf16()).sum()
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
}
