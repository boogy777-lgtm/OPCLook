# Статус

**Текущая работа:** пункты аудита `docs/audit-2026-10-27.md` закрыты и сведены
в `ua-discovery-tree` (PR #3); статус каждого пункта — в таблице аудита,
подробности потоков — `docs/reports/{ua,app}.md` и `docs/devlog.md`.

| Поток | Пункты | Состояние |
|---|---|---|
| X1 Linux-стенд | `scripts/check-linux.sh` | сведено |
| UA backend | U1–U5, U8, U12, U13, U15 | сведено |
| App/UI | U5–U7, U9–U11, U13, U14, D4, D5, A1–A13, K1, K2 | сведено |
| DA | D1, D2, D3, D6, D7 | сведено; COM-часть проверяется только CI под Windows |

**Открыто (пробелы):**
- A9: локализация текста ошибок бэкенда (UA/DA) — нужен общий формат кодов
  ошибок в `EventKind`; 4 `eprintln!` в Windows-only части `mdns.rs`.
- U15 (UI-часть): комбинированный поиск всё ещё шлёт `SearchUaServers` на
  `127.0.0.1` и `localhost` (бэкенд LDS-опрос уже дедуплицирован); нет поля
  «LDS-хост» в диалоге.
- U13 (подписи): текст «Доверять сертификату» не говорит «только на эту сессию».
- Коммит `8319e34` содержит 14 пунктов потока app — откат по одному пункту
  только вручную.

**Сводная проверка (измерено, Linux):** `scripts/check-linux.sh` на сведённой
ветке — 500 passed, 0 failed, 24 ignored.

**Linux-проверка (X1):** `scripts/check-linux.sh` — `fmt`, `check --all-targets`,
`test --lib` и интеграционные тесты на `x86_64-unknown-linux-gnu`.
Измерено: 364 passed, 0 failed, 23 ignored (lib 242 passed; tests/*.rs 122
passed, 23 ignored — 22 требуют живой DA/UA-сервер, 1 Windows-only инвентарь
адаптеров). На Linux DA-воркер — заглушка, отвечающая `Error` на каждую команду.

**Блокеры:** Windows-тулчейна в облачной среде нет — финальная сборка и
`cargo test` только в CI на PR. Push и слияние в main разрешены человеком (2026-10-27).

**Не проверено компиляцией (X1, Windows-сборка `i686-pc-windows-msvc`):**
- `Cargo.toml`: `windows`/`windows-core` перенесены в `[target.'cfg(windows)'.dependencies]`,
  добавлен `[target.'cfg(not(windows))'.dependencies] eframe` с `x11`; `Cargo.lock` пополнен
  зависимостями Linux-бэкенда (удалений нет).
- `src/bff/vt.rs` (новый): `vt_name` вынесена из `da_variant.rs` без изменений;
  `da_variant.rs` делает `pub use super::vt::vt_name;` (путь `da_variant::vt_name` прежний).
- `src/bff/mod.rs`, `src/opc/mod.rs`: `#[cfg(windows)]` на модули `da`, `da_variant`, `opc::bindings`.
- `src/bff/mdns.rs`, `src/da_route.rs`: только `cfg_attr(not(windows), allow(dead_code…))`.
- Всё это на Windows должно собираться как раньше, но не скомпилировано — проверит CI.
