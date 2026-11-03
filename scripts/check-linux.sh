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

echo "==> verify installed locked UA SDK source"
python3 docs/testing/test-ua-sdk-source.py
python3 docs/testing/ua-sdk-source.py

echo "==> cargo check --all-targets ($TARGET)"
cargo check --all-targets --target "$TARGET" -p opclook -p opclook-cli -p opclook-engine -p opclook-backend -p opclook-contract -p opclook-locale

echo "==> build GUI and standalone CLI"
cargo build --target "$TARGET" -p opclook -p opclook-cli
cargo build --target "$TARGET" -p opclook-cli
python3 scripts/check-cli-boundary.py "$TARGET"

echo "==> shared crate and CLI tests ($TARGET)"
cargo test --target "$TARGET" -p opclook-engine -p opclook-backend -p opclook-contract -p opclook-locale -p opclook-cli

echo "==> GUI library tests ($TARGET)"
cargo test --target "$TARGET" -p opclook --lib

echo "==> GUI integration tests ($TARGET)"
cargo test --target "$TARGET" -p opclook --test '*'

echo "==> OK: Linux-проверка зелёная"
