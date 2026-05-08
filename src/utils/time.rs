pub fn parse_duration(s: &str) -> std::time::Duration {
    let s = s.trim();
    if let Some(s) = s.strip_suffix("ms") {
        std::time::Duration::from_millis(s.parse().unwrap_or(0))
    } else if let Some(s) = s.strip_suffix('s') {
        std::time::Duration::from_secs(s.parse().unwrap_or(0))
    } else if let Some(s) = s.strip_suffix('m') {
        std::time::Duration::from_secs(s.parse::<u64>().unwrap_or(0) * 60)
    } else if let Some(s) = s.strip_suffix('h') {
        std::time::Duration::from_secs(s.parse::<u64>().unwrap_or(0) * 3600)
    } else {
        std::time::Duration::from_secs(s.parse().unwrap_or(0))
    }
}
