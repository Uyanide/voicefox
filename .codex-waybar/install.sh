#!/usr/bin/env bash

set -Eeuo pipefail

readonly SOURCE_DIR="/home/emo/code/voicefox/.codex-waybar"
readonly TARGET_DIR="/home/emo/.config/waybar"
readonly LOG_FILE="/tmp/voicefox-waybar.log"

stamp=$(date +%Y%m%d-%H%M%S)
backup_dir="$TARGET_DIR/backups/$stamp"

rollback() {
    printf '\nWaybar failed to start. Rolling back from %s\n' "$backup_dir" >&2
    install -m 0644 "$backup_dir/config.jsonc" "$TARGET_DIR/config.jsonc"
    install -m 0644 "$backup_dir/style.css" "$TARGET_DIR/style.css"
    install -m 0755 "$backup_dir/scripts/command-center.sh" \
        "$TARGET_DIR/scripts/command-center.sh"
    pkill -x waybar 2>/dev/null || true
    sleep 1
    setsid -f waybar >"$LOG_FILE" 2>&1
    printf 'Rollback completed.\n' >&2
}

jq empty "$SOURCE_DIR/config.jsonc"
bash -n "$SOURCE_DIR/command-center.sh"

python3 - "$SOURCE_DIR/style.css" <<'PY'
import sys
import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gtk

provider = Gtk.CssProvider()
provider.load_from_path(sys.argv[1])
PY

mkdir -p "$backup_dir/scripts"
cp -a "$TARGET_DIR/config.jsonc" "$backup_dir/config.jsonc"
cp -a "$TARGET_DIR/style.css" "$backup_dir/style.css"
cp -a "$TARGET_DIR/scripts/command-center.sh" \
    "$backup_dir/scripts/command-center.sh"

install -m 0644 "$SOURCE_DIR/config.jsonc" "$TARGET_DIR/config.jsonc"
install -m 0644 "$SOURCE_DIR/style.css" "$TARGET_DIR/style.css"
install -m 0755 "$SOURCE_DIR/command-center.sh" \
    "$TARGET_DIR/scripts/command-center.sh"

pkill -x waybar 2>/dev/null || true
sleep 1
: >"$LOG_FILE"
setsid -f waybar >"$LOG_FILE" 2>&1
sleep 6

if ! pgrep -x waybar >/dev/null; then
    sed -n '1,260p' "$LOG_FILE" >&2
    rollback
    exit 1
fi

printf 'Backup: %s\n' "$backup_dir"
printf 'Waybar process:\n'
pgrep -a -x waybar
printf '\nStartup log:\n'
sed -n '1,260p' "$LOG_FILE"
