# Статус

**2026-10-28, ветка refactor/layers-headless:** первый этап проверки
`docs/refactoring-plan.md` выполнен частично; статусы B1–B20 и оставшиеся
проверки перечислены в `docs/refactoring-status.md`. GUI DA-подготовка теперь
проходит через ограниченный сервис Engine; Linux live UA CLI проверки прошли.
Полный гейт нового head выполняется. GitHub Linux и Windows CI для `2a6d017`
прошли; Windows runtime/UAC/DA и остальные этапы плана не проверены или не
завершены.

**Текущая работа:** пункты аудита `docs/audit-2026-10-27.md` закрыты и сведены
в `main` (PR #3, PR #4); статус каждого пункта — в таблице аудита,
подробности потоков — `docs/reports/{ua,app,rest}.md` и `docs/devlog.md`.

| Поток | Пункты | Состояние |
|---|---|---|
| X1 Linux-стенд | `scripts/check-linux.sh` | сведено |
| UA backend | U1–U5, U8, U12, U13, U15 | сведено |
| rest | A9, U15 (UI), U13 (подписи), F1 | сведено (PR #4) |
| App/UI | U5–U7, U9–U11, U13, U14, D4, D5, A1–A13, K1, K2 | сведено |
| DA | D1, D2, D3, D6, D7 | сведено; COM-часть проверяется только CI под Windows |
| Отзыв владельца | V1, V2, K3, V3 | сведено (PR #5) |
| Отзыв владельца | P1 (теги большого дерева не обновлялись) | сведено (PR #6) |
| UI-полировка | UI1–UI10 (потоки table, nav, theme) | сведено в main, PR #7 (`d85c744`) |

**Открыто (пробелы):**
- F1: что именно упало в единичном прогоне Windows-CI в PR #3, не подтверждено
  (лога не было); найденная гонка `CoInitializeSecurity` закрыта в 0e475a4.

**Решения владельца (2026-10-27):**
- Клиент обнаружения (`bff/ua/discovery.rs::build_discovery_client`) остаётся с
  `trust_server_certs(true)`: сертификаты серверов, к которым он обращался при
  поиске, сохраняются в общем PKI-хранилище `trusted`. Не менять.
- Коммит `8319e34` (14 пунктов потока app) остаётся как есть; историю `main` не
  переписываем.

**UI1–UI10 после сведения (измерено, Linux):** 588 passed, 0 failed, 24 ignored.
Отчёты потоков — `docs/reports/ui-{table,nav,theme}.md`.

**Отзыв владельца (V1, V2, K3, V3), измерено на Linux:** 534 passed, 0 failed,
24 ignored. Не скомпилировано на Linux: ветка `Value::Struct` в
`bff/da_variant.rs::default_vt` — проверит CI под Windows.

**Сводная проверка (измерено, Linux):** `scripts/check-linux.sh` на сведённой
ветке — 500 passed, 0 failed, 24 ignored; после потока `rest` (A9, U15, U13, F1) —
522 passed, 0 failed, 24 ignored.

**Linux-проверка (X1):** `scripts/check-linux.sh` — `fmt`, `check --all-targets`,
`test --lib` и интеграционные тесты на `x86_64-unknown-linux-gnu`.
Измерено: 364 passed, 0 failed, 23 ignored (lib 242 passed; tests/*.rs 122
passed, 23 ignored — 22 требуют живой DA/UA-сервер, 1 Windows-only инвентарь
адаптеров). На Linux DA-воркер — заглушка, отвечающая `Error` на каждую команду.

**Ограничения среды:** Windows i686 std установлен; Windows-хоста и MSVC SDK
для полноценной компоновки/запуска GUI здесь нет, их проверяет CI. Слияние PR #7
выполнено; работа над новым планом продолжается.

**Не проверено компиляцией (X1, Windows-сборка `i686-pc-windows-msvc`):**
- `Cargo.toml`: `windows`/`windows-core` перенесены в `[target.'cfg(windows)'.dependencies]`,
  добавлен `[target.'cfg(not(windows))'.dependencies] eframe` с `x11`; `Cargo.lock` пополнен
  зависимостями Linux-бэкенда (удалений нет).
- `src/bff/vt.rs` (новый): `vt_name` вынесена из `da_variant.rs` без изменений;
  `da_variant.rs` делает `pub use super::vt::vt_name;` (путь `da_variant::vt_name` прежний).
- `src/bff/mod.rs`, `src/opc/mod.rs`: `#[cfg(windows)]` на модули `da`, `da_variant`, `opc::bindings`.
- `src/bff/mdns.rs`, `src/da_route.rs`: только `cfg_attr(not(windows), allow(dead_code…))`.
- Всё это на Windows должно собираться как раньше, но не скомпилировано — проверит CI.
