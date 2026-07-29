use std::fs;
use std::path::PathBuf;

use lx_core::model::config::{CURRENT_CONFIG_VERSION, Config, ThemeConfig};
use lx_core::model::source::SourceId;

const VERSION_1_DEFAULT_SOURCES: &[SourceId] = &[
    SourceId::Kw,
    SourceId::Kg,
    SourceId::Tx,
    SourceId::Wy,
    SourceId::Mg,
];

/// 加载配置：优先读用户配置文件，否则用默认值
pub fn load(custom_path: &str) -> anyhow::Result<(Config, PathBuf)> {
    let config_path = resolve_config_path(custom_path);

    match fs::read_to_string(&config_path) {
        Ok(content) => {
            let mut config: Config = toml::from_str(&content)?;
            if migrate_legacy_config(&mut config) {
                save(&config, &config_path)?;
            }
            Ok((config, config_path))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // 配置文件不存在，写入默认配置
            let config = Config::default();
            let toml_str = toml::to_string_pretty(&config)?;
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&config_path, toml_str)?;
            Ok((config, config_path))
        }
        Err(error) => Err(error.into()),
    }
}

fn migrate_legacy_config(config: &mut Config) -> bool {
    let mut changed = migrate_legacy_theme(config);
    if config.version < 1 {
        if config.source.enabled == [SourceId::Kw] {
            config.source.enabled = VERSION_1_DEFAULT_SOURCES.to_vec();
        }
        config.version = 1;
        changed = true;
    }
    if config.version < 2 {
        if same_sources(&config.source.enabled, VERSION_1_DEFAULT_SOURCES) {
            config.source.enabled.push(SourceId::Bili);
        }
        config.version = 2;
        changed = true;
    }
    if config.version < 3 {
        // 旧版本曾把 local_music.enabled 的默认值保存为 false，导致已经
        // 配置过目录的用户升级后不会自动扫描。只在版本迁移时恢复一次；
        // 版本 3 之后用户主动关闭仍会被保留。
        if !config.local_music.paths.is_empty() && !config.local_music.enabled {
            config.local_music.enabled = true;
        }
        config.version = 3;
        changed = true;
    }
    if config.version < 4 {
        // 通知原先位于 [ui]，迁移到独立的 [notification]，保留用户已有
        // 的开关和停留时间；缺失字段继续使用新的默认值。
        if let Some(enabled) = config.ui.show_notifications {
            config.notification.in_app = enabled;
        }
        if let Some(timeout) = config.ui.notification_timeout {
            config.notification.in_app_timeout = timeout;
        }
        config.ui.show_notifications = None;
        config.ui.notification_timeout = None;
        config.version = 4;
        changed = true;
    }
    debug_assert!(config.version <= CURRENT_CONFIG_VERSION);
    changed
}

fn same_sources(left: &[SourceId], right: &[SourceId]) -> bool {
    left.len() == right.len() && right.iter().all(|source| left.contains(source))
}

fn migrate_legacy_theme(config: &mut Config) -> bool {
    let theme = &config.theme;
    let uses_original_defaults = theme.accent.eq_ignore_ascii_case("cyan")
        && theme.text.eq_ignore_ascii_case("white")
        && theme.muted.eq_ignore_ascii_case("dark_gray")
        && theme.border.eq_ignore_ascii_case("cyan");
    if uses_original_defaults {
        config.theme = ThemeConfig::default();
    }
    uses_original_defaults
}

/// 获取配置文件路径: ~/.config/voicefox/config.toml
pub fn config_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voicefox");
    dir.join("config.toml")
}

fn resolve_config_path(custom_path: &str) -> PathBuf {
    let custom_path = custom_path.trim();
    if custom_path.is_empty() {
        return config_path();
    }
    if let Some(relative) = custom_path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(relative);
    }
    PathBuf::from(custom_path)
}

/// 保存配置到文件
pub fn save(config: &Config, path: &std::path::Path) -> anyhow::Result<()> {
    let toml_str = toml::to_string_pretty(config)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, toml_str)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load, migrate_legacy_config};
    use lx_core::model::config::{CURRENT_CONFIG_VERSION, Config};
    use lx_core::model::source::SourceId;

    #[test]
    fn migrates_the_original_default_theme_to_mocha() {
        let mut config = Config {
            version: 0,
            ..Config::default()
        };
        config.theme.accent = "cyan".into();
        config.theme.text = "white".into();
        config.theme.muted = "dark_gray".into();
        config.theme.border = "cyan".into();

        migrate_legacy_config(&mut config);

        assert_eq!(config.theme.base, "#1e1e2e");
        assert_eq!(config.theme.accent, "#cba6f7");
    }

    #[test]
    fn expands_the_original_kw_only_source_default_once() {
        let mut config = Config {
            version: 0,
            ..Config::default()
        };
        config.source.enabled = vec![SourceId::Kw];

        assert!(migrate_legacy_config(&mut config));
        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(config.source.enabled, SourceId::all_online());

        config.source.enabled = vec![SourceId::Kw];
        assert!(!migrate_legacy_config(&mut config));
        assert_eq!(config.source.enabled, vec![SourceId::Kw]);
    }

    #[test]
    fn preserves_a_custom_source_selection_during_version_two_migration() {
        let mut config = Config {
            version: 1,
            ..Config::default()
        };
        config.source.enabled = vec![SourceId::Kg, SourceId::Wy];

        assert!(migrate_legacy_config(&mut config));
        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(config.source.enabled, vec![SourceId::Kg, SourceId::Wy]);
    }

    #[test]
    fn reenables_existing_local_music_paths_during_version_three_migration() {
        let mut config = Config {
            version: 2,
            ..Config::default()
        };
        config.local_music.paths = vec!["/music".to_string()];
        config.local_music.enabled = false;

        assert!(migrate_legacy_config(&mut config));
        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert!(config.local_music.enabled);
    }

    #[test]
    fn preserves_local_music_disable_after_version_three_migration() {
        let mut config = Config::default();
        config.local_music.paths = vec!["/music".to_string()];
        config.local_music.enabled = false;

        assert!(!migrate_legacy_config(&mut config));
        assert!(!config.local_music.enabled);
    }

    #[test]
    fn migrates_legacy_ui_notification_options() {
        let mut config = Config {
            version: 3,
            ..Config::default()
        };
        config.ui.show_notifications = Some(false);
        config.ui.notification_timeout = Some(9);

        assert!(migrate_legacy_config(&mut config));
        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert!(!config.notification.in_app);
        assert_eq!(config.notification.in_app_timeout, 9);
        assert_eq!(config.ui.show_notifications, None);
        assert_eq!(config.ui.notification_timeout, None);
    }

    #[test]
    fn loads_partial_config_with_defaults() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "voicefox-partial-config-{}-{}.toml",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, "[player]\nvolume = 42\n").unwrap();

        let (config, _) = load(path.to_str().unwrap()).unwrap();

        assert_eq!(config.player.volume, 42);
        assert_eq!(config.player.engine, "mpv");
        assert!(config.player.remember_playback_state);
        assert_eq!(config.network.timeout, 15);
        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(config.source.enabled, SourceId::all_online());
        let _ = std::fs::remove_file(path);
    }
}
