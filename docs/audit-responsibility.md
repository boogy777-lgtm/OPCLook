# Аудит ответственности исходников

**Дата:** 2026-10-26  
**Область:** `src/`, `tests/`  
**Статус:** аудит завершён. Production-код не перемещался; добавлен только
тестовый DFB-seam (`tests/support/dfb.rs`).

> **Обновление 2026-10-26 (network-binding):** монолитные `src/app.rs`,
> `src/ui.rs`, `src/bff/ua.rs` уже разбиты на `src/app/`, `src/ui/`,
> `src/bff/ua/`. Добавлены `src/da_route.rs` (DA route steering) и
> workspace-crate `nethelper/` (`opcda-nethelper.exe`). Метрики и ссылки на
> строки ниже — исторический снимок до разбиения.

## 1. Вердикт

Критерий «нет god object; папки и файлы имеют свою ответственность» **сейчас не
выполняется**. Три файла являются god object/module:

1. `src/app.rs` — 4 176 строк: модель состояния, lifecycle сессий, reducer
   событий, сетевой поиск, таблица/дерево, запись, CSV, диалоги и test hooks.
2. `src/ui.rs` — 4 967 строк: меню, панели, дерево, таблица, все диалоги,
   файловые операции и CSV.
3. `src/bff/ua.rs` — 4 234 строки: worker loop, сессии, security, discovery,
   LDS/mDNS merge, сканер, browse/read/write, подписки и конвертация значений.

Дополнительно размыты границы у `src/net.rs` (модель + Windows API + состояние
приложения) и `src/bff/model.rs` (контракты + display-семантика + значения).

Положительная часть: направление зависимостей чистое и ацикличное
(`ui -> app -> bff`, `app -> settings/net/columns/journal`, `bff -> model/opc`).
`Gateway` уже является правильной границей между UI и протоколами. Проблема не
в отсутствии архитектуры, а в том, что несколько модулей выросли далеко за
пределы одной причины изменения.

## 2. Метрики и ответственность

| Файл | Строк | Фактические ответственности | Вердикт |
|---|---:|---|---|
| `src/app.rs` | 4 176 | state root, сессии, reconnect, reducer, поиск, таблица/дерево, запись, CSV, диалоги, test hooks | **God object** |
| `src/ui.rs` | 4 967 | chrome, панели, дерево, таблица, все диалоги, файлы, CSV, цвета, test hooks | **God module** |
| `src/bff/ua.rs` | 4 234 | worker, sessions, security, discovery, LDS/mDNS, scanner, IO, values | **God module** |
| `src/net.rs` | 1 501 | network domain, Windows inventory, app state | Смешаны слои |
| `src/bff/model.rs` | 1 376 | command/event, endpoint, security, value, quality | Широкий, но связный словарь |
| `src/i18n.rs` | 2 129 | RU/EN/ZH dictionary | Одна ответственность |
| `src/bff/mdns.rs` | 719 | Windows DNS-SD | Одна ответственность |
| `src/bff/da.rs` | 965 | DA COM worker | Одна ответственность |
| `src/opc/bindings/*` | 6 243 | сгенерированные COM-биндинги | Исключение: generated |

Пороговые значения — не догма. Но файл > 2 000 строк, который одновременно
меняется по 3+ независимым причинам, уже нельзя считать модулем с одной
ответственностью.

## 3. Находки

### F1. `App` — god object (P0)

`src/app.rs`:

- `444–700` — структуры диалогов (`ConnectDialog`, `Dialogs`, write/columns/
  options/about/message/confirm).
- `701–750` — `App` с десятками полей: сессии, строки, выделение, дерево,
  журнал, диалоги, CSV job, add-children job, сеть, write-progress.
- `751–3266` — **2 516 строк одного `impl App`**: connect/disconnect/reconnect,
  event reducer, search, scan, browse, add/remove, tree, write, CSV, options,
  seed/test hooks.
- `3267–3731` — free functions: name/URL helpers, value parsing, scan merge,
  operation labels, font installation.
- `3732–4169` — unit tests, включая `seed_*` и test-only hooks.

Причины изменения, не связанные друг с другом:

1. UI state (выделение, диалоги, фокус).
2. Протокольный lifecycle и reconnect.
3. Event reducer и request correlation.
4. Сетевой discovery/scan.
5. Таблица/дерево/команды пользователя.
6. Write validation и value parsing.
7. CSV import/export.
8. Шрифты и eframe adapter.
9. Test hooks, которые компилируются в production.

Это классический god object: любой новый сценарий с высокой вероятностью
добавляет поле и метод в `App`, увеличивая связанность. Следствие — конфликты
при параллельной работе и высокая цена навигации.

**Целевое разбиение:**

```text
src/app/
  mod.rs          App + композиция состояния, публичные команды
  state.rs        SessionView, Row, TreeItem, WriteProgress
  dialogs.rs      *Dialog + Dialogs
  sessions.rs     connect/disconnect/remove/reconnect
  events.rs       on_event + request correlation + reducer
  search.rs       DA/UA discovery, LDS/mDNS, subnet scan
  table.rs        rows, selection, add/remove, write
  tree.rs         tree actions, add-children
  import.rs       CSV import/export
  present.rs      name/URL/operation labels (или view/)
  test_support.rs #[cfg(test)] seed/on_event hooks
```

### F2. `ui.rs` — god module (P0)

`src/ui.rs`:

- `100–420` — test hooks и painting helpers.
- `423–2354` — каркас UI: меню, tab strip, status bar, bottom panels,
  navigation, tree, table, shortcuts, status popup.
- `2355–3971` — все диалоги: connect, add signal, write, columns, options,
  about, message, confirm.
- `3972–4445` — file I/O, tree CSV export/import, CSV import job.
- `4446–4967` — цвета и мелкие helpers.

Модуль одновременно является renderer, controller (мутирует `App`), file I/O и
CSV-парсером. Даже внутри renderer смешаны chrome, navigation, table и dialogs.
Любое изменение диалога или таблицы затрагивает один и тот же файл.

**Целевое разбиение:**

```text
src/ui/
  mod.rs          draw() + композиция
  menu.rs
  status.rs
  servers.rs      tab strip + server list
  tree.rs
  table.rs
  panels.rs       script/journal/bottom tabs
  dialogs/
    mod.rs
    connect.rs
    add_signal.rs
    write.rs
    columns.rs
    options.rs
    csv_import.rs
    about.rs
    message.rs
  files.rs        save/load script/journal/tree CSV
  colors.rs
  test_hooks.rs   #[cfg(test)]
```

Правило: render-функции получают `&App`/`&SessionView` и возвращают action
enum; `&mut App` применяется в одной точке кадра. Сейчас это уже сделано для
tree/table, но не для диалогов.

### F3. `bff/ua.rs` — god module (P0)

`src/bff/ua.rs`:

- `64–230` — security policy/mode resolution.
- `244–1334` — worker loop, command dispatch, session registry, timeouts,
  subscriptions, value events.
- `1393–1617` — discovery: `FindServers`, `GetEndpoints`.
- `1618–1968` — subnet scanner: targets, concurrency, progress, cancel.
- `1969–2166` — LDS/mDNS aggregation and verification.
- `2224–2425` — connect/session bootstrap/identity.
- `2426–3041` — browse, read, write, test.
- `3042–3260` — `Variant`/`DataValue` conversion.

Это не один worker, а пять подсистем в одном файле. Сканер и discovery не
должны знать о внутренностях `UaSession`; сейчас они физически рядом и делят
private helpers.

**Целевое разбиение:**

```text
src/bff/ua/
  mod.rs          spawn + worker loop + public contract
  worker.rs       command queue/priority/cancel
  session.rs      UaSession + connect/teardown/identity
  security.rs     resolve_security + policy/mode types
  discovery.rs    FindServers/GetEndpoints
  network.rs      LDS + mDNS merge
  scanner.rs      N+1 targets + progress/cancel
  io.rs           browse/read/write/test
  values.rs       Variant/DataValue conversion
```

`bff/ua.rs` можно оставить как `pub use ua::*` shim на время миграции, чтобы не
ломать импорты.

### F4. `net.rs` и `model.rs` — смешанные слои (P1)

`src/net.rs` содержит:

- чистые модели (`AdapterId`, `AdapterInfo`, `NetworkLease`, `SubnetSpec`);
- Windows API (`GetAdaptersAddresses`, `GetIfEntry2`);
- app-side `NetworkState`;
- тесты всех трёх групп.

**Целевое:**

```text
src/net/
  mod.rs       pub use
  model.rs     AdapterId/Info/Lease/SubnetSpec
  windows.rs   inventory (cfg(windows))
  state.rs     NetworkState
```

`src/bff/model.rs` — единый словарь BFF. Его ширина оправдана, но внутри есть
разные причины изменения: protocol contracts, endpoint identity, security,
quality/value. Рекомендуется разделить без изменения публичного re-export:

```text
src/bff/model/
  mod.rs       pub use
  endpoint.rs
  command.rs
  event.rs
  security.rs
  value.rs
  quality.rs
```

Это не P0, потому что файл остаётся связным словарём, а не объектом с
поведением. Но 1 376 строк и `Value: Display` с русской презентационной строкой
— это boundary leak: display должен жить в `ui`/`view`.

### F5. Папки не отражают домены (P1)

Сейчас корень `src/` смешивает:

- `app.rs`, `ui.rs` — крупные домены без папок;
- `net.rs` — домен без папки;
- `settings.rs`, `columns.rs`, `journal.rs` — нормальные leaf-модули;
- `bff/` — хорошая граница, но `ua.rs` внутри перегружен;
- `bin/probe_*` — диагностические утилиты рядом с production binary.

**Целевое дерево:**

```text
src/
  app/
  ui/
  bff/
    model/
    da/
    ua/
    mdns.rs
    net.rs
  net/
  settings.rs
  columns.rs
  journal.rs
  i18n.rs
  constants.rs
  bin/            только production entrypoints/probes
```

`src/opc/bindings` не трогать: это generated-код с отдельной лицензией.

### F6. Тесты: нет единого DFB-seam (P1)

Сейчас каждый integration test заново пишет `connect`, `disconnect`,
`wait_for_event`, `send`, `drain`:

- `tests/ua_smoke.rs`, `tests/ua_lifecycle.rs`, `tests/ua_large.rs`,
  `tests/ua_types.rs`, `tests/ua_security.rs`, `tests/ua_subscription.rs`,
  `tests/ua_search.rs`, `tests/gateway_shutdown.rs`.

Это не ошибка логики, но нарушает критерий «один функциональный блок с ручками
на вход и выход»: граница теста размазана по helper-функциям, а часть тестов
лезет во внутренние `App::seed_*`/`on_event_for_test_*`, которые являются
production-public.

**DFB-модель для тестов:**

- один блок — `Gateway`;
- вход — `Command`;
- выход — `Event`;
- trait `Dfb` — единственный контракт теста;
- тест не получает доступ к `sessions`, `pending_requests`, worker internals;
- UI-тесты остаются на `egui_kittest` и `App` (это другой класс блока).

Реализация: `tests/support/dfb.rs`. Trait:

```rust
pub trait Dfb {
    type Input;
    type Output;

    fn input(&mut self, input: Self::Input) -> Result<(), String>;
    fn output(&mut self, timeout: Duration) -> Option<Self::Output>;
    fn shutdown(&mut self);
}
```

`Gateway` реализует `Dfb<Input = Command, Output = Event>`. Новые protocol-тесты
должны использовать только `Dfb` и не импортировать `Gateway` напрямую, кроме
создания блока. Это аналог DFB: один вызов/вход, один выход, без доступа к
внутренностям.

Ограничение: не тащить этот trait в production API и не делать `dyn Dfb` там,
где достаточно concrete `Gateway`. Trait существует для тестового seam.

### F7. Test hooks в production (P1)

`src/app.rs` публикует `new_test`, `on_event_for_test`, `tick_for_test`,
`seed_session`, `seed_tree_item`, `seed_row`, `seed_writable_row`,
`pending_request_ids_for_test`. Это production-public surface, хотя
используется только тестами. Рекомендация: `#[cfg(test)]` или feature
`test-hooks`; UI-хуки уже `#[cfg(test)]`.

После введения DFB для protocol-тестов `seed_*` останутся только для
UI-тестов; их можно оставить под `#[cfg(test)]`.

### F8. Что уже правильно и не надо ломать

- `Gateway` — facade/command boundary; не заменять на actor framework.
- `Command`/`Event` — правильный protocol-neutral контракт.
- `bff/da_variant` — чистый adapter COM ↔ Value.
- `i18n::Tr` — compile-checked dictionary; не разносить строки по файлам.
- `columns`, `journal`, `settings`, `constants` — нормальные leaf-модули.
- Acyclic dependencies: UI не импортирует worker internals.
- `egui_kittest` — правильный harness для immediate-mode UI; не заменять
  view-model слоем.

## 4. Целевая структура

```text
src/
  app/
    mod.rs
    state.rs
    dialogs.rs
    sessions.rs
    events.rs
    search.rs
    table.rs
    tree.rs
    import.rs
    present.rs
  ui/
    mod.rs
    menu.rs
    status.rs
    servers.rs
    tree.rs
    table.rs
    panels.rs
    files.rs
    colors.rs
    dialogs/
      mod.rs
      connect.rs
      add_signal.rs
      write.rs
      columns.rs
      options.rs
      csv_import.rs
      about.rs
      message.rs
  bff/
    mod.rs
    model/
    da/
    ua/
    mdns.rs
    net.rs
  net/
    mod.rs
    model.rs
    windows.rs
    state.rs
  settings.rs
  columns.rs
  journal.rs
  i18n.rs
  constants.rs
  opc/bindings/   generated, не трогать
tests/
  support/
    mod.rs
    dfb.rs
  ua_*.rs
  ui_interactions.rs
```

## 5. План миграции

Порядок выбран так, чтобы каждый шаг компилировался и не менял поведение.

### Phase 0 — test seam (сделано)

- Добавлен `tests/support/dfb.rs`.
- `Gateway` реализует `Dfb`.
- `tests/gateway_shutdown.rs` переведён на trait.
- Новые protocol-тесты обязаны использовать `Dfb`.

### Phase 1 — низкий риск

1. Вынести `*Dialog`/`Dialogs` из `app.rs` в `app/dialogs.rs`.
2. Вынести `compose_server_name`, URL helpers, `operation_label` в
   `app/present.rs`.
3. Вынести file/CSV I/O из `ui.rs` в `ui/files.rs` + `ui/dialogs/csv_import.rs`.
4. Разделить `net.rs` на `net/{model,windows,state}.rs`.
5. Закрыть test hooks под `#[cfg(test)]`.

Проверка: `cargo test --lib`, `cargo test --test ui_interactions`,
`cargo test --test no_hardcoded_ui_strings`, `cargo check --all-targets`.

### Phase 2 — `app` по сценариям

1. `app/state.rs` — типы состояния.
2. `app/events.rs` — `on_event`, request correlation, reducer.
3. `app/sessions.rs` — connect/disconnect/reconnect/remove.
4. `app/search.rs` — DA/UA discovery/scan.
5. `app/table.rs`, `app/tree.rs`, `app/import.rs`.

Правило: методы `impl App` могут жить в дочерних модулях; доступ к private
полям остаётся в пределах `app::*`. Публичный API `App` не меняется.

### Phase 3 — `ui` по панелям и диалогам

1. Вынести dialogs (самый большой и независимый кусок).
2. Вынести chrome/status/servers/panels.
3. Вынести tree/table; сохранить существующие action enums.
4. `ui/mod.rs` оставить только `draw()` и композицию.

### Phase 4 — `bff/ua` по подсистемам

1. `security.rs` — чистые функции.
2. `values.rs` — конвертация.
3. `discovery.rs`, `network.rs`, `scanner.rs`.
4. `session.rs`, `io.rs`, `worker.rs`.

`bff/ua.rs` на время миграции превращается в shim с `pub use`.

### Phase 5 — документация и guardrails

- Обновить `README.md` (module table) и `docs/audit-architecture.md`.
- Добавить тест-страж: список файлов > 1 000 строк должен быть явно
  whitelisted с причиной; новый файл сверх порога — fail.
- Правило code review: один файл — одна причина изменения; новый публичный
  test hook — только `#[cfg(test)]`.

## 6. Критерии готовности

- Ни один production-файл не совмещает UI, protocol I/O и persistence.
- `app.rs`, `ui.rs`, `bff/ua.rs` отсутствуют как god-файлы; их функции
  распределены по подпапкам.
- Нет циклов зависимостей; `ui` не импортирует worker internals.
- Protocol-тесты используют `Dfb` (`Command` → `Event`) и не читают внутреннее
  состояние `Gateway`.
- UI-тесты используют kittest и `App`, но не protocol workers.
- Все существующие тесты и release build зелёные.
- Generated `src/opc/bindings` не перемещается и не редактируется вручную.

## 7. Не делать

- Не заменять `Gateway` на actor framework.
- Не вводить `trait Backend`/`Box<dyn>` над двумя протоколами: enum dispatch
  уже проверяется компилятором.
- Не вводить DI-контейнер, EventBus, Repository для settings.
- Не переносить `i18n` строки в файлы панелей.
- Не делать `Dfb` частью production API.
- Не дробить файлы механически по 200 строк без ответственности.
