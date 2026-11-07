//! Small text helpers shared by the service and the window.

/// Collapses whitespace into single spaces and caps the length, so a huge
/// clipboard payload never has to be laid out in full.
pub fn summarise(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if count >= limit {
            out.push('…');
            break;
        }
        if pending_space {
            out.push(' ');
            count += 1;
            pending_space = false;
        }
        out.push(character);
        count += 1;
    }
    out
}

pub fn human_time(created_at: u64, now: u64) -> String {
    let seconds = now.saturating_sub(created_at) / 1000;
    match seconds {
        0..=5 => "刚刚".to_string(),
        6..=59 => format!("{seconds} 秒前"),
        60..=3599 => format!("{} 分钟前", seconds / 60),
        3600..=86_399 => format!("{} 小时前", seconds / 3600),
        _ => format!("{} 天前", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarise_collapses_whitespace_and_caps_length() {
        assert_eq!(summarise("  hello \n\t world  ", 100), "hello world");
        assert_eq!(summarise("abcdef", 3), "abc…");
        assert_eq!(summarise("   ", 10), "");
    }

    #[test]
    fn human_time_uses_readable_units() {
        assert_eq!(human_time(1_000, 3_000), "刚刚");
        assert_eq!(human_time(0, 30_000), "30 秒前");
        assert_eq!(human_time(0, 120_000), "2 分钟前");
        assert_eq!(human_time(0, 7_200_000), "2 小时前");
        assert_eq!(human_time(0, 180_000_000), "2 天前");
    }
}
