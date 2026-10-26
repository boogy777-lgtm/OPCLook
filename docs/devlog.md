# Дев-лог

## 2026-10-27 — аудит и дерево серверов UA
- Закрыто: T1–T8 (дерево сервер → URL → безопасность, слияние результатов
  поиска, DiscoveryUrls из FindServers). Измерено: 16 тестов стенда
  (ua_tree, i18n, merge) зелёные; живой FindServers против sample-сервера
  async-opcua вернул 1 сервер, 12 endpoints (4 из них Aes — клиент их не знает).
- Открыто: 35 пунктов аудита (U1–U15, D1–D7, A1–A13).
- Не проверено: полная сборка под `i686-pc-windows-msvc` (нет тулчейна).

## 2026-10-27 — X1: проверка проекта на Linux-хосте
- Закрыто: X1. `scripts/check-linux.sh` зелёный (`fmt`, `check --all-targets`,
  `test --lib`, `test --test '*'`, target `x86_64-unknown-linux-gnu`,
  `CARGO_TARGET_DIR=/home/claude/target-linux`; `.cargo/config.toml` не менялся).
- Измерено: lib 242 passed / 0 failed / 0 ignored; интеграционные 122 passed /
  0 failed / 23 ignored. Ignored: 22 теста `#[ignore]` «нужен живой DA/UA-сервер»
  (были и раньше), 1 новый `cfg_attr(not(windows), ignore)` —
  `network_tab_refresh_button_queries_windows_inventory` (нужен
  GetAdaptersAddresses). Тесты `da_variant` (roundtrip VARIANT/SAFEARRAY) на Linux
  не компилируются вместе с модулем — они только Windows.
- Решения: (1) на не-Windows `bff::da` — заглушка `src/bff/da_unsupported.rs`
  (тот же `spawn`), отвечает `EventKind::Error { «OPC DA поддерживается только в
  Windows (COM)» }` на каждую команду, как и Windows-воркер молчит на
  `Disconnect`/`ConfigureUaSubscription`/`ScanUaNetwork`/`CancelUaScan`;
  (2) `vt_name` вынесена в `bff/vt.rs`, чтобы UI не зависел от COM-модуля;
  (3) `net.rs`, `bff/net.rs`, `bff/mdns.rs`, `da_route.rs` уже имели
  `cfg(windows)`/`cfg(not(windows))`, менялись только атрибуты `allow(dead_code)`.
- Побочный эффект тестов: ui-тесты создают каталог `opcda-egui/pki` в рабочей
  папке (не в `.gitignore`) — не коммитить.
- Не проверено: сборка под `i686-pc-windows-msvc` (нет тулчейна), CI.
