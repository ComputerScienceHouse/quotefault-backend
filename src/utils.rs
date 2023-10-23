pub fn is_valid_username(username: &str) -> bool {
    username.len() <= 32 && username.chars().any(|x| x.is_ascii_alphanumeric())
}
