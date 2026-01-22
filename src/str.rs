pub fn escape_quote<const QUOTE_CHAR: char>(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for ch in value.chars() {
        if ch == QUOTE_CHAR {
            escaped.push('\\');
        }

        escaped.push(ch);
    }

    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    mod escape_quote {
        use super::*;

        #[test]
        fn succeeds() {
            assert_eq!(escape_quote::<'"'>("hello"), "hello");
            assert_eq!(escape_quote::<'"'>("he\"ll\"o"), "he\\\"ll\\\"o");
            assert_eq!(escape_quote::<'"'>("he'll'o"), "he'll'o");
            assert_eq!(escape_quote::<'\''>("he'll'o"), "he\\'ll\\'o");
        }
    }
}
