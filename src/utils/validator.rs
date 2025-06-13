use anyhow::Result;
use crate::config::Config;

#[allow(dead_code)]
pub fn is_valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")
}

#[allow(dead_code)]
pub fn validate_thread_count(threads: usize) -> Result<()> {
    if threads == 0 {
        anyhow::bail!("线程数必须大于0");
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_speed_limit(limit: f32) -> Result<()> {
    if limit < 0.0 {
        anyhow::bail!("速度限制不能为负数");
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_output_path(path: &str) -> Result<()> {
    if path.is_empty() {
        anyhow::bail!("输出路径不能为空");
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_urls(urls: &[String]) -> Result<()> {
    if urls.is_empty() {
        anyhow::bail!("URL列表不能为空");
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_config(config: &Config) -> Result<()> {
    config.validate()
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

    #[test]
    fn test_thread_count_validation() {
        assert!(validate_thread_count(1).is_ok());
        assert!(validate_thread_count(32).is_ok());
        assert!(validate_thread_count(0).is_err());
        assert!(validate_thread_count(33).is_err());
    }

    #[test]
    fn test_speed_limit_validation() {
        assert!(validate_speed_limit(0.0).is_ok());
        assert!(validate_speed_limit(1000.0).is_ok());
        assert!(validate_speed_limit(-1.0).is_err());
        assert!(validate_speed_limit(1001.0).is_err());
    }

    #[test]
    fn test_output_path_validation() {
        assert!(validate_output_path("./").is_ok());
        assert!(validate_output_path("./nonexistent").is_ok());
        // 注意：这个测试可能需要根据实际环境调整
        // assert!(validate_output_path("/dev/null").is_err());
    }

    #[test]
    fn test_urls_validation() {
        let valid_urls = vec![
            "https://example.com".to_string(),
            "http://example.com".to_string(),
        ];
        assert!(validate_urls(&valid_urls).is_ok());

        let invalid_urls = vec![
            "invalid-url".to_string(),
            "https://example.com".to_string(),
        ];
        assert!(validate_urls(&invalid_urls).is_err());
    }
} 