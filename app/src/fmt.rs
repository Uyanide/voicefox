//! 时间格式化助手：各页面/组件共用，避免七份各写一份还带细微差异。

use std::time::Duration;

/// `Duration` → `mm:ss`。
///
/// 超过一小时也按总分钟数展示（`75:30` 而不是 `1:15:30`），与进度条、
/// 状态栏、表格时长列的既有显示习惯一致。
pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_minutes_and_seconds() {
        assert_eq!(format_duration(Duration::from_secs(0)), "00:00");
        assert_eq!(format_duration(Duration::from_secs(65)), "01:05");
        assert_eq!(format_duration(Duration::from_secs(4530)), "75:30");
    }
}
