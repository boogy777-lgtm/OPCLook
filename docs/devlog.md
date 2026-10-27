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

## 2026-10-27 — DA: D1, D2, D3, D6, D7
- Закрыто (ветка `fix/da`, по коммиту на пункт; D6 и D7 в одном): D1 — CLSID
  удалённого сервера берётся у удалённого OPCEnum (`IOPCServerList`, затем
  `IOPCServerList2`), затем из кэша обнаружения, затем из локального реестра
  (`bff/da_clsid.rs`); D2 — маршрутизатор `bff/da_router.rs`: поток на сессию и
  на обнаружение (не более 8), `catch_unwind`, ровно один ответ на каждую
  команду в очереди, ограниченный join при выходе; D3 — `CoInitializeSecurity`
  (`RPC_C_AUTHN_LEVEL_NONE`, `RPC_C_IMP_LEVEL_IDENTIFY`) один раз на процесс,
  при `RPC_E_TOO_LATE` причина идёт в журнал; D6 — успех «Теста связи» только при
  `RUNNING`; D7 — «версия сервера X.Y.Z, групп на сервере: N».
- Измерено: `scripts/check-linux.sh` зелёный: lib 264 passed (было 242: +5 `da_clsid`,
  +2 `da_status`, +15 `da_router`), интеграционные 122 passed без изменений; всего 386 passed, 0 failed.
- Решения: при отказе `CoInitializeSecurity` работа продолжается (процесс мог уже
  настроить безопасность до старта потока); неудача создания потока сессии
  отвечает `Error` на `Connect`.
- Не проверено: COM-часть `bff/da.rs` (D1 glue, D2 `DaHost`, D3, D6/D7) не
  компилируется на Linux — проверит CI `i686-pc-windows-msvc`. Тестов для D3 нет.
- Не сделано: A9 (bff/da) — единый формат кодов ошибок затрагивает `model.rs`,
  `i18n.rs` и `ua`; решение за согласованием с агентами `ua`/`app`.

## 2026-10-27 — сведение потоков ua, app, da
- Сведены ветки `fix/da`, `fix/app`, `fix/ua` в `ua-discovery-tree`; конфликт
  только в таблице аудита (статусы объединены построчно).
- Добавлено координатором: K1 — Connect, истёкший на стороне приложения, сразу
  шлёт воркеру `Disconnect` (иначе поздно подключившаяся сессия оставалась на
  сервере; регрессионный тест падал без фикса); CI выводит упавшие
  интеграционные тесты аннотацией.
- Измерено: Linux-гейт 500 passed / 0 failed / 24 ignored. CI под Windows на
  `e29c748` (до слияния ua) — зелёный; на `0fa55b7` один раз падал шаг
  интеграционных тестов при том же коде — причина не установлена (лог
  недоступен), повтор зелёный.
- Открыто: см. «Открыто» в `docs/status.md`.
