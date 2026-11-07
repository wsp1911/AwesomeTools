const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{} B", bytes);
    }
    let digits = if value >= 100.0 { 0 } else { 1 };
    format!("{:.*} {}", digits, value, UNITS[unit])
}

/// Thousands separators, matching what the UI showed before.
pub fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn format_percent(part: u64, total: u64) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    let ratio = part as f64 / total as f64 * 100.0;
    if ratio > 0.0 && ratio < 0.1 {
        return "<0.1%".to_string();
    }
    let digits = if ratio >= 10.0 { 0 } else { 1 };
    format!("{:.*}%", digits, ratio)
}

pub fn format_duration(ms: u64) -> String {
    if ms == 0 {
        return "0 秒".to_string();
    }
    let seconds = ms as f64 / 1000.0;
    if seconds < 1.0 {
        return format!("{} 毫秒", ms);
    }
    if seconds < 60.0 {
        let digits = if seconds < 10.0 { 1 } else { 0 };
        return format!("{:.*} 秒", digits, seconds);
    }
    let minutes = (seconds / 60.0).floor() as u64;
    let rest = (seconds % 60.0).round() as u64;
    format!("{} 分 {} 秒", minutes, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(151 * 1024 * 1024 * 1024), "151 GB");
    }

    #[test]
    fn counts_get_thousand_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(1234567), "1,234,567");
    }

    #[test]
    fn percentages_keep_one_decimal_until_ten() {
        assert_eq!(format_percent(0, 0), "0%");
        assert_eq!(format_percent(1, 3), "33%");
        assert_eq!(format_percent(1, 200), "0.5%");
        assert_eq!(format_percent(1, 10), "10%");
        assert_eq!(format_percent(1, 100000), "<0.1%");
    }
}
