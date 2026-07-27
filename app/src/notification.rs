//! 统一通知服务：TUI toast 由 AppContext 管理，桌面通知在后台发送。

use lx_core::events::Notification;

#[derive(Clone)]
pub struct DesktopNotifier {
    tx: tokio::sync::mpsc::UnboundedSender<Notification>,
}

impl DesktopNotifier {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        spawn_desktop_worker(rx);
        Self { tx }
    }

    pub fn send(&self, notification: Notification) {
        let _ = self.tx.send(notification);
    }
}

#[cfg(target_os = "linux")]
fn spawn_desktop_worker(mut rx: tokio::sync::mpsc::UnboundedReceiver<Notification>) {
    tokio::spawn(async move {
        let mut connection = None;
        let mut replaced_id = 0u32;

        while let Some(notification) = rx.recv().await {
            if connection.is_none() {
                match zbus::Connection::session().await {
                    Ok(value) => connection = Some(value),
                    Err(error) => {
                        tracing::debug!("desktop notification D-Bus unavailable: {error}");
                        continue;
                    }
                }
            }

            let Some(conn) = connection.as_ref() else {
                continue;
            };
            match send_linux_notification(conn, &notification, replaced_id).await {
                Ok(id) => {
                    if notification.replace_previous {
                        replaced_id = id;
                    }
                }
                Err(error) => {
                    tracing::warn!("desktop notification failed: {error}");
                    connection = None;
                }
            }
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn spawn_desktop_worker(mut rx: tokio::sync::mpsc::UnboundedReceiver<Notification>) {
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
}

#[cfg(target_os = "linux")]
async fn send_linux_notification(
    connection: &zbus::Connection,
    notification: &Notification,
    replaced_id: u32,
) -> zbus::Result<u32> {
    use std::collections::HashMap;

    use zbus::zvariant::OwnedValue;

    let proxy = zbus::Proxy::new(
        connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await?;
    let title = notification
        .title
        .clone()
        .unwrap_or_else(|| level_title(notification));
    let icon = notification.icon.clone().unwrap_or_else(default_app_icon);
    let replace_id = if notification.replace_previous {
        replaced_id
    } else {
        0
    };
    let actions: Vec<String> = Vec::new();
    let hints: HashMap<String, OwnedValue> = HashMap::new();

    proxy
        .call(
            "Notify",
            &(
                "voicefox",
                replace_id,
                icon,
                title,
                notification.message.as_str(),
                actions,
                hints,
                5_000i32,
            ),
        )
        .await
}

#[cfg(target_os = "linux")]
fn level_title(notification: &Notification) -> String {
    use lx_core::events::NotificationLevel;

    match notification.level {
        NotificationLevel::Info => "voicefox".to_string(),
        NotificationLevel::Success => "voicefox · 成功".to_string(),
        NotificationLevel::Warn => "voicefox · 警告".to_string(),
        NotificationLevel::Error => "voicefox · 错误".to_string(),
    }
}

#[cfg(target_os = "linux")]
fn default_app_icon() -> String {
    let mut candidates = Vec::new();
    if let Some(data_home) = dirs::data_dir() {
        candidates.push(
            data_home
                .join("icons/hicolor/512x512/apps/voicefox.png")
                .to_string_lossy()
                .into_owned(),
        );
    }
    candidates.extend([
        "/usr/local/share/icons/hicolor/512x512/apps/voicefox.png".to_string(),
        "/usr/share/icons/hicolor/512x512/apps/voicefox.png".to_string(),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../icons/1.png").to_string(),
    ]);

    candidates
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .unwrap_or_else(|| "voicefox".to_string())
}
