// Check if the string is a valid number
pub(crate) fn is_valid_number(s: &str) -> bool {
    s.chars()
        .enumerate()
        .all(|(i, c)| c.is_digit(10) || (i == 0 && c == '-'))
}
