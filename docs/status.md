# Статус

**2026-10-28, UI modules:** connection results/security, tree rows, table cells and table action handling separated. Linux 219 library, 149 interaction (one Windows-only ignored), 20 table tests; Windows i686 all-target typecheck passed. Five real headless UI scenes rerendered and visually inspected in `docs/screenshots/after-h/`. Exact-head CI and Windows runtime remain open.

**2026-10-28, GUI coordinator:** event routing separated into discovery, session, data and operation-error handlers; write batch owns its counter transitions and session lookups have accessors. Linux 219 GUI library and 149 interaction tests (one Windows-only ignored) passed before a final equivalent protocol lookup simplification; Windows i686 all-target typecheck passed after it. UI module decomposition, exact-head CI and runtime checks remain open.

**2026-10-28, network journal:** network control messages and blocked-save warning localized in RU/EN/ZH; script save success is logged only on success. Linux GUI 219 library and 149 interaction tests (one Windows-only ignored), Windows i686 all-target typecheck passed. GUI decomposition, exact-head CI and runtime checks remain open.

**2026-10-28, settings groups:** ViewPrefs, UaPrefs and NetworkPrefs now own related preferences while retaining the legacy flat JSON schema. Linux GUI 217 library, 149 interaction (one Windows-only ignored), 14 theme tests and Windows i686 all-target typecheck passed. Remaining GUI decomposition, localization, exact-head CI and Windows runtime remain open.

**2026-10-28, settings and late pushes:** Settings returns load notices and save errors without shared mutable diagnostics; exit reports save and shutdown failures in the native error dialog. Queued unsolicited data is rejected while disconnected or connecting. Linux GUI 216 library tests, 149 interaction tests (one Windows-only ignored), 14 theme and isolation test passed; Windows i686 all-target typecheck passed. Settings grouping, GUI structure, exact-head CI and Windows runtime remain open.

**2026-10-28, GUI link loss:** unified UI cleanup for lost links, including dead worker dispatch and pending writes; Linux GUI library 214/214 and Windows i686 workspace all-target typecheck passed. Remaining GUI/Settings refactoring, runtime Windows/DA/UAC, and exact-head CI remain open.

**2026-10-28, backend G/H:** GUID/subnet, UA parsed-node, worker helpers and session modules, one-session DA worker, contract model and network module splits implemented. Linux backend 218/218 including live UA, contract/locale tests, CLI boundary check and Windows i686 workspace all-target typecheck passed; exact-head CI still needs completion. GUI/Settings structural work, Windows runtime and real DA/UAC remain open. Current matrix: `docs/refactoring-status.md`; older entries below are historical.

**2026-10-28, F:** GUI caches, bounded array expansion and virtualized journal implemented; measured before/after and five images reviewed. Remaining G/H, Windows runtime, physical DA/UAC and exact-head CI remain open. Details: `docs/refactoring-status.md` and `docs/reports/refactor-frame-baseline.md`.

**2026-10-28, предыдущее состояние:** B и E реализованы, включая B15 live
proxy и CSV diagnostics; пять headless PNG просмотрены. F baseline измерен: 10k статично 2918.9 мкс медиана / 4472.3 мкс p95,
100 Values + кадр 3312.2 / 6089.1 мкс; отдельно журнал 5000 записей
75120.0 / 101021.1 мкс (CPU wall, не GPU FPS). F оптимизация,
G/H, Windows runtime, физические DA/UAC и финальный exact-head CI остаются
открытыми. Подробности — в `docs/refactoring-status.md` и `docs/devlog.md`.

**Windows CI:** исправление владения VARIANT прошло полный Windows CI на
предыдущем head `7f`; для `5217` завершились Linux и Windows CI. Текущий head требует
отдельного полного exact-head CI перед слиянием.

**2026-10-28, продолжение этапа D:** COM/WinAPI ownership и safety lint
продвинуты; подробная матрица реализации, проверок и открытых пунктов — в
`docs/refactoring-status.md`. Локальная Windows i686 проверка типов и Clippy не
являются выполнением Windows тестов. Текущий exact-head CI, реальный DA/UAC и
остальные этапы плана ещё обязательны.

**2026-10-28, ветка refactor/layers-headless:** первый этап проверки
`docs/refactoring-plan.md` выполнен частично; статусы B1–B20 и оставшиеся
проверки перечислены в `docs/refactoring-status.md`. GUI DA-подготовка теперь
проходит через ограниченный сервис Engine; Linux live UA CLI проверки прошли.
Полный Linux gate для `a7d07ab` прошёл; Windows i686 workspace прошёл
проверку типов с test-only флагами. Финальный CI нового head, Windows
runtime/UAC/DA и остальные этапы плана ещё не завершены.

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

**2026-10-28, типизация Browse:** фильтр классов узлов передаётся enum от
состояния GUI до команды; остальные A/C и финальные проверки остаются открыты.
Подробности — `docs/refactoring-status.md`.
Синтетический Views root централизован; прочие пункты A/C остаются открыты.
Частичные старые settings получают значения по умолчанию, интервалы ограничены
при загрузке; остальная типизация настроек ещё не выполнена.
Порядок колонок типизирован и legacy-формы обрабатываются при загрузке;
другие пункты A/C остаются открыты.
Качество строки отделено от серверного Quality; остальные A/C ещё открыты.
Язык настроек типизирован с сохранением legacy-кодов; остальное A/C открыто.
Инварианты refresh/lifetime и UA keepalive проверяются при компиляции;
оставшиеся пункты A/C открыты.
Настройки AdapterId/IpAddr типизированы; остальные пункты A/C открыты.
DA VarType типизирован; UA URL/security/индексы и role-ID ещё открыты.
UA URL: первый общий parser подключён частично; discovery merge и остальные
пункты C, role ID и финальный CI ещё открыты.
UA discovery/scanner используют общую URL identity, LDS aliases различают порты;
часть merge ещё повторно разбирает URL, security и NonZero индексы открыты.
Discovery URL теперь проверены и типизированы на входе; advertised security,
NonZero индексы и role IDs остаются открыты.
Ключ UA-сессии различает валидный и невалидный URL enum; оставшиеся C/A открыты.
Исправлено смещение эпох DA FILETIME; Windows runtime и остальные этапы открыты.
Advertised security типизировано с сохранением unknown; NonZero индексы и
остальные этапы плана остаются открыты.
Interface индексы и scope типизированы NonZero; реальный Windows runtime и
остальные этапы плана остаются открыты.
Duration reconnect/timeout и проверки инвариантов выполнены; роли ID и этапы
B/E/F/G/H, Windows runtime и финальный exact-head CI остаются открыты.
Backoff покрывает весь диапазон Duration; итоговые CI и другие этапы открыты.
Проверка UA authority согласована с запрещёнными SDK host символами; итоговый
CI и остальные этапы открыты.
Матрица C и передача оставшихся A/B/E/F/G/H — в refactoring-status; зелёный
CI 557fca20 относится к D hotfix, не к текущему head.

**Этап A, частично:** ScanId типизирован как ненулевой идентификатор; после последнего значения запуск сканирования сообщает об исчерпании в диалоге и журнале. Остальные роли, Origin/Owner и финальный CI остаются открыты.

Счётчики сессий и сигналов проверяют исчерпание без переполнения; отдельные типы SessionId/ItemKey и Origin/Owner всё ещё открыты.

Исчерпание request IDs при DA-подготовке теперь возвращает типизированную локализованную ошибку; отказ Disconnect сохраняет ожидающие запросы. Типизация RequestId/Origin/Owner ещё открыта.

ItemKey теперь ненулевой отдельный тип до backend/CLI и явных UA/DA wire handles; SessionId, RequestId, Origin/Owner и финальный CI ещё открыты.

Backend unit tests теперь по умолчанию используют временный PKI-каталог; Windows runtime остаётся открытым.

**2026-10-28, A RequestId/Origin:** ненулевой RequestId, явный Origin и необязательный токен команды проходят через Engine, воркеры и клиенты; отказы отправки не оставляют ложных ожиданий в UI. Измеренные проверки и оставшиеся SessionId/Owner, B/E/F/G/H и точный Windows CI указаны в `docs/refactoring-status.md`. Текущая Windows проверка локально только typecheck; runtime и физический DA/UAC остаются открыты.

**2026-10-28, тема A завершена:** SessionId и Owner перенесены через все слои; discovery больше не использует фиктивную сессию. RequestId/Origin и остальные роли A сохранены. Измеренные проверки, ограничения Windows typecheck и оставшиеся B/E/F/G/H перечислены в `docs/refactoring-status.md`; финальный exact-head CI, Windows runtime, DA/UAC и PNG ещё не проверены.

**2026-10-28, этап B частично:** подтверждение хранит обязательное действие в
`Option<ConfirmRequest>`, выбор UA identity и вкладки подключения типизирован.
Linux GUI unit 198/198 и UI interaction 141/141 (1 Windows-only ignored),
Linux all-targets и Windows i686 workspace all-targets typecheck прошли; Windows
проверка использовала test-only пропуск ресурсов и helper manifest. Матрица и
ограничения — в `docs/refactoring-status.md`. Остальные B/E/F/G/H, PNG, 10k
baseline, Windows runtime, физические DA/UAC и финальный exact-head CI открыты.

**2026-10-28, подэтап B2:** UA discovery и фаза подтверждения/выполнения
подсети представлены enum, ожидающие Auto запросы непусты и коррелированы;
исход ScanEnd исключает сочетание отмены и ошибки. DA и UA discovery не
сбрасывают запросы друг друга. Проверки и открытые ограничения — в
`docs/refactoring-status.md` и `docs/devlog.md`. Остальные пункты B/E/F/G/H,
PNG, измеренный 10k baseline, Windows runtime, реальный DA/UAC и финальный CI
остаются открыты.

**2026-10-28, подэтап B3:** DA результат и выбор объединяют server и host,
CSV import имеет явный lifecycle, EndpointSpec хранит Auto напрямую с чтением
старых absent/null значений. Измеренные проверки и ограничения приведены в
`docs/refactoring-status.md`; остальные B/E/F/G/H, PNG, 10k baseline, Windows
runtime, физические DA/UAC и финальный exact-head CI остаются открыты.

CSV dialog читает job без checked `expect`; регрессия отмены и завершения и
Windows GUI typecheck прошли. Остальные ограничения B3 сохраняются.

**2026-10-28, подэтап B4:** DA и UA представлены `SessionKind`, UA-поля
доступны только в UA-варианте, endpoint обновляется согласованно с видом.
Проверки и ограничения — в `docs/refactoring-status.md`; остальные B/E/F/G/H,
PNG, измеренный 10k baseline, Windows runtime, физические DA/UAC и финальный
exact-head CI остаются открыты.

B5 частично: helper lifecycle типизирован, pending launch/process сохраняется
при deadline; Linux route tests 17/17. IPC/errors и Windows runtime ещё открыты.

B5/IPC: общий tagged Request сохраняет protocol v2, typed IPC ошибки передают
причину в snapshot и локализованное сообщение; подробные проверки и пределы —
в refactoring-status. Windows runtime и реальный DA/UAC не проверены.


**2026-10-28, подэтап B6:** JournalMessage различает обычный текст и ключ с
аргументами; запись не хранит повторную русскую копию. Переключение RU/EN/ZH,
экспорт и повторная загрузка покрыты тестами. Полные данные проверок и
открытые ограничения — в `docs/refactoring-status.md`. E/F/G/H, измеренный
10k baseline, PNG, Windows runtime, физические DA/UAC и итоговый exact-head CI
остаются открытыми.

**2026-10-28, CSV diagnostics:** обе формы импорта сообщают в журнале строки
с ошибками, число сообщений ограничено; номера учитывают многострочные поля и
разные переводы строки. Локальный Windows i686 typecheck не заменяет runtime.
B15 live proxy и прочие открытые проверки плана остаются открыты.

**2026-10-28, B15 live:** Linux sample server + TCP proxy подтвердили
timeout и прерывание Disconnect (2/2); Windows i686 backend typecheck прошёл,
Windows runtime и финальный exact-head CI остаются открыты.

**2026-10-28, F измерительный стенд (промежуточный):** добавлен headless
`App::update` для 10 000 строк и отдельного журнала на 5000 записей; 120 кадров
и обновление 100 значений через обработчик событий прошли Linux smoke в debug.
Это ещё не оптимизированный baseline. Рендер PNG, оптимизированные измерения,
F/G/H, Windows runtime и итоговый exact-head CI остаются открыты.

**2026-10-28, визуальная проверка:** пять PNG `App::update` (10k RU,
ошибка DA helper RU, security/identity UA ZH, сеть RU, подтверждение RU)
получены headless и просмотрены. Ошибка DA искусственно задана; реальный
Windows helper/UAC/DA остаётся непроверенным. Оптимизированный F baseline
ещё измеряется.


**2026-10-28, H — граница компиляции GUI:** normal dependencies GUI теперь
contract, engine и locale; backend доступен напрямую только как dev dependency
для низкоуровневых интеграционных тестов. Engine явно предоставляет безопасные
сетевые и сертификатные метаданные, VT и типизированную проверку UA security;
Gateway доступен только через test-support. Guard проверяет граф normal
зависимостей. Остальные структурные задачи H и Settings, оптимизации F/G,
Windows runtime, физический DA/UAC и финальный exact-head CI остаются открыты.
