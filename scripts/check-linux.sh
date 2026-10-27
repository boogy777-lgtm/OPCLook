#!/usr/bin/env bash
# Проверка проекта на Linux-хосте (без Windows-тулчейна).
#
# Целевая платформа проекта — i686-pc-windows-msvc, а `.cargo/config.toml`
# фиксирует её как target по умолчанию, поэтому здесь target задан явно.
# Windows-код (COM/WinAPI) на Linux заменён заглушками «не поддерживается на
# этой платформе»; логика (app, ui, bff/ua, i18n, settings, csv, model,
# net-логика) компилируется и тестируется как есть. Тесты, требующие Windows,
# помечены `#[cfg_attr(not(windows), ignore = "Windows-only: ...")]`.
#
# Использование: scripts/check-linux.sh   (из любого каталога)
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

TARGET="x86_64-unknown-linux-gnu"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/claude/target-linux}"

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "==> cargo check --all-targets ($TARGET)"
cargo check --all-targets --target "$TARGET" -p opcda-egui

echo "==> cargo test --lib ($TARGET)"
cargo test --target "$TARGET" -p opcda-egui --lib

echo "==> cargo test integration tests/*.rs ($TARGET)"
cargo test --target "$TARGET" -p opcda-egui --test '*'

echo "==> OK: Linux-проверка зелёная"
