use regex::Regex;

/// 校验URL格式（支持http/https/ftp）
pub fn is_valid_url(url: &str) -> bool {
    let re = Regex::new(r"^(https?|ftp)://[\w\-]+(\.[\w\-]+)+(:\d+)?(/[\w\-./?%&=]*)?$")
        .unwrap();
    re.is_match(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_url_validation() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://example.com"));
        assert!(!is_valid_url("invalid-url"));
    }
} 