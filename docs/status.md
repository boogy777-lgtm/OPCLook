# Статус

**Текущая работа:** исправление пунктов аудита `docs/audit-2026-10-27.md`
(35 открытых: U1–U15, D1–D7, A1–A13). T1–T8 закрыты в `ua-discovery-tree`.

| Поток | Владелец | Пункты | Состояние |
|---|---|---|---|
| UA backend | агент `ua` | U1, U2, U3, U4, U5 (бэкенд), U8, U12, U13, U15, A9 (bff/ua) | в работе |
| App/UI | агент `app` | U5 (UI), U6, U7, U9, U10, U11, U14, D4, D5, A1–A8, A10–A13, A9 (app) | в работе |
| DA | агент `da` | D1, D2, D3, D6, D7, A9 (bff/da) | в работе |
| Linux-стенд | агент `linux` | X1: `scripts/check-linux.sh` (fmt, check, тесты на Linux) | сделано в ветке `fix/linux-check`, ждёт слияния; Windows-сборка не проверена |

**Linux-проверка (X1):** `scripts/check-linux.sh` — `fmt`, `check --all-targets`,
`test --lib` и интеграционные тесты на `x86_64-unknown-linux-gnu`.
Измерено: 364 passed, 0 failed, 23 ignored (lib 242 passed; tests/*.rs 122
passed, 23 ignored — 22 требуют живой DA/UA-сервер, 1 Windows-only инвентарь
адаптеров). На Linux DA-воркер — заглушка, отвечающая `Error` на каждую команду.

**Блокеры:** Windows-тулчейна в облачной среде нет — финальная сборка и
`cargo test` только в CI на PR. Push требует решения человека.

**Не проверено компиляцией (X1, Windows-сборка `i686-pc-windows-msvc`):**
- `Cargo.toml`: `windows`/`windows-core` перенесены в `[target.'cfg(windows)'.dependencies]`,
  добавлен `[target.'cfg(not(windows))'.dependencies] eframe` с `x11`; `Cargo.lock` пополнен
  зависимостями Linux-бэкенда (удалений нет).
- `src/bff/vt.rs` (новый): `vt_name` вынесена из `da_variant.rs` без изменений;
  `da_variant.rs` делает `pub use super::vt::vt_name;` (путь `da_variant::vt_name` прежний).
- `src/bff/mod.rs`, `src/opc/mod.rs`: `#[cfg(windows)]` на модули `da`, `da_variant`, `opc::bindings`.
- `src/bff/mdns.rs`, `src/da_route.rs`: только `cfg_attr(not(windows), allow(dead_code…))`.
- Всё это на Windows должно собираться как раньше, но не скомпилировано — проверит CI.
