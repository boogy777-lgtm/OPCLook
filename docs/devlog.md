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

## 2026-10-27 — поток rest (A9, U15 UI, U13 подписи, F1)
- A9: бэкенд UA/DA больше не строит русские строки: `enum BackendMsg` (158 вариантов,
  подстановки именованные, вложенные причины — `Box`) в `bff/model.rs`; `i18n::backend_text`
  (RU/EN/ZH, исчерпывающий `match`); приложение локализует событие при приёме
  (`app/events.rs`, причина строки — `DataValue.reason`). Тексты библиотек/ОС идут
  подстановками без перевода. Тест-сканер `no_hardcoded_ui_strings` запрещает кириллицу
  в литералах `src/bff/**` вне тестов (сканер научен пропускать сырые строки и
  символьные литералы в тестовых модулях).
- U15: комбинированный поиск — один loopback-запрос; поле «LDS-хост» (`host[:порт]`,
  в `settings.json`); `SearchUaNetwork { bind, lds }`, `LdsTarget` со своей привязкой.
- U13: подпись «…(только на эту сессию)» и подсказка во всех трёх языках.
- F1: настройки UI-тестов — на поток (`settings::thread_dir`), PKI — на процесс
  (`use_private_pki_dir`), создание клиента под замком (`build_client`).
- Измерено: Linux-гейт 500 → 522 passed, 0 failed, 24 ignored.
- Не проверено компиляцией: Windows-части A9 (см. «Открыто» в `docs/status.md`).

## 2026-10-27 — CI под Windows на PR #4
- Аннотации CI теперь снимают ANSI-цвета cargo и показывают хвост лога, если
  шаблоны не совпали (ba6f807) — без этого ошибка компиляции не была видна.
- A9, Windows-часть: `da.rs:578` клал `String` в `Result<_, BackendMsg>` (E0308)
  — 20e5291.
- F1/D3: `CoInitializeSecurity` один раз на процесс (0e475a4). Каждый `Gateway`
  вызывал его заново; второй получал `RPC_E_TOO_LATE` и писал строку журнала,
  которую ловил `gateway_shutdown::send_after_shutdown_returns_error` в зависимости
  от порядка старта двух параллельных тестов. Регрессионный тест — сам этот
  Windows-тест (код только под Windows).

## 2026-10-27 — решения владельца
- Сертификаты, найденные при поиске серверов, остаются в общем `trusted` (без изменений в коде).
- Сборный коммит `8319e34` остаётся; история `main` не переписывается.

## 2026-10-27 — отзыв владельца: теги по endpoint, клавиатура, структуры (V1, V2, K3, V3)
- Закрыто: V1 — таблица показывает теги активной вкладки (каждый endpoint — свой
  список); V2 — UA-сессия = URL + политика + режим, вкладка показывает безопасность;
  K3 — окно записи открывается с выделенным значением, Tab/F6, Ctrl+Tab,
  Ctrl+PgUp/PgDn, Ctrl+N, исправлен Ctrl+A → Delete после фокуса в дереве; V3 —
  структуры UA раскрываются в дерево полей.
- Решения: (1) без активной вкладки таблица показывает все строки — строка не
  пропадает без вкладки-владельца; (2) Tab забирается у egui только когда клавиатурой
  владеет дерево или таблица, иначе egui ходит по виджетам как раньше; (3) типы
  структур грузятся один раз после подключения, только из пространств имён ≠ 0
  (стандартные типы клиент знает сам), бюджет 30 с; отказ — строка в журнале,
  структуры остаются текстом; (4) запись структур по-прежнему не поддерживается
  (понятная ошибка с именем типа).
- Измерено: регрессионные тесты падали на `main` до исправления — выделение
  («4277» вместо «77»), повтор после ошибки («42abc»), Ctrl+A+Delete (осталось 2
  строки); живой тест V3 без загрузки типов не получает `Value::Struct`. Linux-гейт
  534 passed (lib 381, интеграционные 153), 0 failed, 24 ignored. PNG таблицы с
  раскрытой структурой и двумя вкладками одного сервера просмотрен.
- Не проверено: Windows-сборка `da_variant.rs` (новая ветка `Value::Struct` в
  `default_vt`) — проверит CI; серверы до OPC UA 1.04 (без DataTypeDefinition) —
  структуры у них остаются текстом.

## 2026-10-27 — P1: теги большого дерева перестают обновляться
- Воспроизведено: живой тест, 2000 тегов пачками по 100 (как «добавить
  дочерние»): после 1000-го сервер отвечает `BadTooManyMonitoredItems` на каждый
  вызов CreateMonitoredItems — лимит элементов на одну подписку. Клиент держал одну
  подписку на сессию; такие строки показывали ошибку и, пока подписка активна, не
  опрашивались — значение появлялось только после ручного чтения. «Плавающий»: зависит
  от размера дерева и лимитов конкретного сервера.
- Решения: (1) сверх лимита — следующая подписка с тем же интервалом и обработчиком
  (не более 32 на сессию); (2) `BadTooManyOperations` — размер вызова делится пополам и
  запоминается для сессии; (3) если новая подписка не создаётся или ничего не берёт —
  теги «только опрос»: событие `PollOnly`, строка в журнале, приложение читает их
  каждым интервалом опроса и при активной подписке; переподключение решает заново;
  (4) удаление и смена интервала/зоны нечувствительности — по каждой подписке.
- Измерено: тест на 2000 тегах падал до исправления (ошибка на ключах 1001–2000);
  после — 1500 тегов через две подписки, 300 тегов при лимите 30 на вызов, 300 при
  одной подписке на 200 (100 «только опрос», сообщены) — все зелёные.

## 2026-10-27 — UI1–UI10: полировка интерфейса (три потока)
- Закрыто: UI1–UI10 (подробности и цифры потоков — `docs/reports/ui-table.md`,
  `ui-nav.md`, `ui-theme.md`). Таблица: плашки качества, нейтральные значения, числа
  справа, подсветка изменения ~1 с, нарисованный треугольник раскрытия, пустая таблица с
  действием. Вкладки/левая панель/дерево: вкладки с подчёркиванием и замком безопасности,
  панель иконок, мягкое выделение сервера, пустое дерево с «Обновить (F5)». Тема/шкала:
  светлая/тёмная/как в системе (настройка), шкала 12/13/15 и отступы 8×6, журнал по
  умолчанию, панель скрипта компактнее.
- Сведение: конфликты только в `docs/audit-2026-10-27.md` (строки пунктов объединены по
  id) и в инициализаторах `Row` (`poll_only` из P1 + `changed_at` из UI4 — оба поля).
  После сведения шаг строк таблицы вырос до 30 px (строка 24 + общий зазор 6) —
  таблица держит свой зазор 2 px (`TABLE_ROW_GAP`).
- Решения потоков, меняющие поведение: `show_log` по умолчанию включён; F5 на пустом
  дереве обновляет корень; «×» неактивной вкладки виден по наведению; тема применяется
  по «Сохранить».
- Измерено: PNG сводного окна в тёмной и светлой темах просмотрены. Цифры гейта —
  в `docs/status.md`.
- Не проверено: Windows-сборка — CI; перетаскивание на пустую таблицу с кнопкой — только
  по построению и старым DnD-тестам.


## 2026-10-28 — начат первый этап Rust-ревью

Проверены находки B1–B20 и оба P; исправления и ограничения перечислены в
`refactoring-status.md`. Измерено на Linux: 398 тестов библиотеки прошли, один
диагностический отфильтрован; integration binaries прошли с отмеченными ignored.
Новый тест TrustLease прошёл отдельно. Windows i686 проверен `cargo check --tests`
с явным пропуском ресурсов/тестового манифеста; исполнение под Windows и COM/UAC
остаются для CI/стенда. Helper теперь проверяет PID обоих концов pipe, SID клиента
и использует BCrypt для имени; незавершённое ограничение времени IPC записано в
статусе. Перенос слоёв ещё не выполнен.


## 2026-10-28 — начало переноса слоёв

Текущие исходники backend перенесены в отдельный крейт вместе с исправлениями
этапа B1–B20; контракт и локализация отделены, Engine обслуживает оба клиента.
Компиляция Linux тестовых целей и отдельного CLI проверена. Измерено: Engine
4 passed, CLI unit 2 passed, GUI `ui_interactions` 138 passed, 1 ignored;
полный гейт и живой CLI ещё ожидают проверки. Блокирующая подготовка DA-маршрутов и helper
остаётся открытой задачей, как и остальные этапы `refactoring-plan.md`.


### 2026-10-28: граница DA и CLI

Маршрутные описания перенесены в contract, платформенный код — в backend;
перенос не устраняет синхронную подготовку в GUI. Измерено на Linux: CLI 4
и Engine 5 тестов прошли; 12 route-тестов backend прошли.
Запись JSON ограничена по фактическим сериализованным байтам; закрытый канал
stdout завершает клиент без диагностической ошибки. Незатребованная ошибка
чтения больше не освобождает слот ожидаемого ответа. CI для `2a6d017`
прошёл на Linux и Windows; аппаратный DA/UAC не проверен.

Helper exchange ограничен 3 секундами и 16 КиБ; потеря pipe инвалидирует
helper-owned leases. Измерено: 14 Linux route-тестов прошли; Windows i686
backend `cargo check --tests` прошёл, реальный pipe/UAC/DA не запускался.

GUI DA discovery/connect переданы Engine и ограниченному route service;
ошибки локализуются из типизированных событий. Измерено на Linux: GUI lib 186,
Engine 11, locale 9 и тест реального кадра App::update прошли. Проверка
реального Windows pipe/UAC/DA и живого CLI остаётся открытой.

Измерено на Linux: 2 live subprocess-теста CLI прошли против sample UA;
проверены browse/read/write/read, три принудительных poll, push после 3 секунд
покоя при timeout 1 секунда, Ctrl+C, отсутствие секретов в ошибке и BrokenPipe.

Таймауты GUI-запросов формирует Engine через типизированные события; поздние
ответы отбрасываются. Измерено: Engine 12, GUI lib 186 и locale 9 прошли.

Engine подтверждает AddItems по результатам каждого ключа и не опрашивает
ошибочные или просроченные добавления; ранний PollOnly и удаление до ответа
покрыты регрессиями. Измерено: 16 тестов Engine и 2 live CLI-теста прошли.
Повторный полный гейт на текущем head выполняется.

Измерено для `a7d07ab`: полный Linux gate прошёл; Windows i686 workspace
`cargo check --all-targets` прошёл с test-only пропуском ресурсов/манифеста.
Финальный Windows CI и реальный DA/UAC ещё не выполнены. PNG GUI не рендерился;
его проверка и настоящий 10k frame/allocation benchmark переданы следующему
UI/performance этапу. Grace-дренирование того же future при timeout сохранить.


2026-10-28, этап D: guard для COM строк, массивов, VARIANT, статуса и группы;
helper token/LocalAlloc/impersonation и системной библиотеки DNS; Windows
safety lint расширен до изменённых платформенных модулей. Добавлены Windows
регрессии B11/B12/B19 и resource cleanup, пока только typecheck. Локально
измерено: locale 9/9, K1 GUI 1/1, diagnostics `cargo check`; финальный CI
текущего head и внешние DA/UAC проверки открыты. См. матрицу
`docs/refactoring-status.md`.

Этап D продолжен: group guard владеет группой до shutdown/drop; BYREF и
неизвестные/несовместимые layout SAFEARRAY отклоняются, глубина вложенности
ограничена 32. Это ограничения форматов, не реализованная поддержка BYREF.
Измерено на Linux: backend lib 189 passed, 0 failed, 0 ignored. Новые Windows
регрессии проверены типами и ожидают выполнения в CI.

Завершение этапа D: SAFEARRAY передаётся в VARIANT только после успешных
Unaccess/Unlock; Drop повторяет неудачные операции и пытается уничтожить массив.
При постоянном сбое OLE освобождение не гарантировано. Windows fault-тесты
пока только typecheck, выполнение ожидает CI; timeout pipe worker ограничен
временем жизни завершающегося helper процесса.

### 2026-10-28 — тип фильтра Browse

Фильтр классов узлов сделан явным enum от состояния сессии до команды и UA
маски. Выбор Variables сохраняет Objects для доступа к вложенным переменным.
Остальные пункты типизации A/C ещё открыты; полный gate и Windows runtime не
считаются выполненными этой локальной проверкой.

Синтетический Views root использует общий идентификатор и метод contract;
контрактный тест прошёл на Linux, Windows GUI проверен только по типам.

Частичные старые settings загружаются с defaults вместо полной потери файла;
сохранённые интервалы по-прежнему ограничиваются при загрузке. Linux regression
1/1 и Windows GUI typecheck прошли; остальные пункты C остаются открыты.

Порядок колонок типизирован при загрузке; legacy quoted/plain значения,
дедупликация, fallback и формат сохранения покрыты settings тестами (46/46
Linux), взаимодействие диалога проверено отдельно (1/1 Linux). Windows —
только typecheck, полный gate не выполнен.

Качество строки теперь различает серверный код и локальные состояния ожидания,
нет данных и ошибки; коды FFFFFFFE/FFFFFFFD больше не совпадают с плейсхолдером.
Linux UI polish 17/17, stale interactions 5/5, CLI unit 5/5, Windows typecheck;
рендер PNG и общий gate остаются открытыми.

Settings хранит типизированный Lang и сохраняет совместимые коды языка;
старые синонимы и неизвестный код проверены на границе. Linux settings 46/46,
меню языка 1/1 и Windows GUI typecheck прошли.

Сервис DA использует общий Duration refresh; отношения lease и UA keepalive
проверяются при компиляции. Linux тест констант 1/1, Windows typecheck прошли;
прочие временные границы и полный gate остаются открыты.

Сетевой выбор настроек хранит AdapterId/IpAddr и разбирает старые строки один
раз при загрузке. Невалидные значения остаются unselected; stale GUID не
удаляется автоматически. Linux settings 46/46, network-tab 5 passed / 1
Windows-only ignored; Windows проверен по типам.

DA VarType отделён от UA типов и использует именованные константы в GUI,
контракте и VT-таблице backend. Проверены Linux VT 1/1, K3 UI 1/1 и Windows
typecheck; реальный COM runtime остаётся открытым.

Первый шаг общего UaUrl проверяет вход до журнала и сохраняет смысл пути,
query и fragment при сравнении; оставшиеся пути обнаружения и объединения ещё
не переведены. SDK не принимает scoped IPv6 URL; это не заявленная новая
поддержка. Полный gate и Windows runtime остаются открытыми.

Windows CI изолировал heap corruption в тесте некорректного layout: Drop
`VARIANT` в extension windows-rs ранее не был учтён. Исправлены двойное
уничтожение в тесте и владение BSTR при повторном чтении SAFEARRAY; тест
повторного чтения добавлен. Windows runtime исправления ещё ожидает CI.

Общий UaUrl используется в дереве, scanner и LDS вводе; идентичность сохраняет
регистр path/query и учитывает адрес и порт. Loopback LDS aliases разделены по
порту. Linux discovery 12/12, scanner 2/2, U15 8/8, UI grouping 2/2; Windows
workspace typecheck прошёл. Полный parse-once merge и остальные пункты C открыты.

Discovery URL проверяются при получении и хранятся UaUrl, с безопасной
диагностикой отброшенных записей; совпадение сессии учитывает host, порт,
path/query и security. Linux discovery 13/13, scanner 2/2, UI identity 2/2,
grouping 2/2, security-filter 10/10, locale 9/9; Windows typecheck прошёл,
реальный runtime и полный gate остаются обязательными.
Ключ URL сессии хранит валидный identity либо исходный invalid, без двух
взаимоисключающих полей. Linux identity 2/2 и Windows GUI typecheck прошли.

Исправлен знак смещения FILETIME→Unix для DA дат; 0 остаётся отсутствующим
временем. Linux checked-conversion regression 1/1; Windows FILETIME wrapper
прошёл проверку типов, runtime ещё нужен в CI.

Advertised policy/mode представляют отсутствие, известное или неизвестное
значение; неизвестные и неродной `#None` не понижаются, scanner сохраняет
несколько вариантов одного URL. Linux UA 62/62, GUI security 10/10, live CLI
2/2, Windows typecheck и Rust 1.95 contract check прошли. Windows runtime и
оставшиеся этапы открыты.

Interface и IPv6 scope типизированы как NonZero; legacy cache сохраняет
числовую форму интерфейсов, нулевой scope использует индекс IPv6 для link-local.
Linux network 35/35, CLI 6/6, settings 46/46, UI network 5 passed / 1 ignored,
Windows typecheck и Rust 1.95 contract check прошли; runtime Windows открыт.

Reconnect и request floors типизированы Duration, арифметика backoff ограничена;
значения settings/wire остаются совместимыми числами. Linux Engine 18/18,
GUI reconnect 3/3, Windows workspace typecheck прошли; общий gate открыт.

Граница Duration backoff исправлена для публичного API с наносекундной базой
и огромным максимумом; Linux regression 1/1 и Windows engine typecheck прошли.

UA URL валидатор отклоняет запрещённые SDK символы host и знак порта;
Linux parity/security test 1/1 и Windows backend typecheck прошли.

Advertised::Known(Auto), если создан вручную, отвергается в resolver и UI;
Linux regressions 1/1 + 1/1 и Windows workspace typecheck прошли. Компактная
матрица C и последующая передача перечислены в refactoring-status. Зелёный
Windows/Linux CI 557fca20 проверил D hotfix, но не последующие C/FILETIME.

**2026-10-28 — A, ScanId:** ненулевой тип проходит через контракт, GUI и scanner; checked successor выдаёт u64::MAX один раз, затем остановка видима в диалоге и журнале (RU/EN/ZH). Измерено: Linux regression исчерпания 1/1, scanner 10/10, stale scan UI 1/1; Linux all-targets check основных пакетов и Windows i686 workspace all-targets check с test-only флагами прошли. Windows runtime, физические DA/UAC/маршруты и PNG не проверены; прочие роли и Origin/Owner открыты.

**2026-10-28 — A, исчерпание GUI:** генерация session/item прекращается после выдачи максимума; ошибка локализована и видна в статусе и журнале. Частичная партия сигналов сохраняет выданные ключи и отправляется; остаток не создаётся. Измерено: Linux две регрессии исчерпания прошли; Windows i686 workspace all-targets typecheck с test-only флагами прошёл. Типизация ролей и реальный Windows runtime остаются открыты.

**2026-10-28 — A, исчерпание request ID:** Engine проверяет наличие ID до отмены маршрутов и ожидающих запросов при Connect/Disconnect; DA-подготовка возвращает отдельную ошибку исчерпания до создания route service. Linux Engine 19/19 и Windows i686 workspace all-targets typecheck с test-only флагами прошли. Числовой RequestId и sentinel Origin/Owner ещё открыты, runtime/финальный CI не проверены.

**2026-10-28 — A, ItemKey:** ненулевой тип вместо u32 по контракту, Engine и GUI/CLI; UA/DA передают числовой handle через get, входящий UA zero отклоняется с локализованной записью в журнал. Checked successor сохраняет ранее исправленное исчерпание. Compile-fail защищает смешение ItemKey/ScanId. Linux GUI exhaustion и headless удаление строки 1/1 каждое; Linux all-targets и Windows i686 workspace all-targets typecheck с test-only флагами прошли. Реальный DA, UAC, Windows runtime и PNG всё ещё не проверены; SessionId/RequestId/Origin/Owner открыты.

Измерено после ItemKey: Linux backend 199/199, Engine 19/19, CLI 6/6; Windows backend/CLI standalone all-targets typecheck без resource skip прошёл. Первый backend run выявил тестовый fallback, вычислявший недопустимый zero; исправлен и полный набор повторно прошёл.

**2026-10-28 — изоляция PKI тестов:** backend unit tests используют временный каталог до инициализации live fixture, а не рабочий каталог/профиль. Linux regression 1/1 и Windows i686 backend tests typecheck без resource skip прошли. Runtime Windows не проверен.

**2026-10-28 — A, RequestId/Origin:** RequestId отделён от ScanId и не допускает zero, внутренние команды могут быть без токена, Event различает ответ и push. Корреляция остаётся одноразовой, timeout несёт исходный токен; проверены push при pending Read, late reply, DA route и exhaustion, включая сохранение работающего Read. UI при отказе отправки освобождает ожидания и не завершает неотправленные действия; принятая часть write batch учитывается до следующей отправки. Исправлен отсутствующий locale sample InvalidItemHandle (9/9). Измерено: Linux backend 200/200, CLI 6/6 + live 2/2, contract 25/25 + doctest 2/2, Engine 20/20, GUI unit 196/196 и headless UI interactions 139/139 (1 ignored); Windows i686 all-targets typecheck с test-only флагом и standalone backend/CLI без него прошли. Реальный Windows runtime/DA/UAC, PNG и exact-head CI не проверены; SessionId/Owner, B/E/F/G/H остаются открыты.

**2026-10-28 — A, SessionId/Owner:** discovery отделён от реальных ненулевых SessionId во всём стеке; checked successor сохраняет выдачу максимума один раз. Pending/таймауты ключуются Owner, backend отклоняет владельца не того вида, discovery timeout не воздействует на session. Engine test recorder фиксирует фактическую отправку ровно одного внутреннего Disconnect с Owner::Session и без RequestId при таймауте Connect. Исправлен найденный GUI эффект: unsolicited Values/Read Error не сбрасывает pending Read. Измерено Linux: backend 202/202, contract 26/26 + 3 doctests, locale 9/9, CLI 6/6 + live 2/2, Engine 22/22, GUI 198/198, UI interactions 139/139 (1 ignored); Windows i686 all-targets typecheck с test-only флагами, backend/CLI standalone без пропуска ресурсов и diagnostics bins Linux/Windows typecheck. A завершён; B/E/F/G/H, финальный CI, Windows runtime, физический DA/UAC, PNG и 10k baseline открыты.

**2026-10-28 — состояния диалогов B (частично).** Подтверждение владеет
обязательным действием до Yes/No; повторный Enter в кадре открытия и Enter с
модификаторами не согласуют запись. UA identity — enum; введённые имя и пароль
остаются в буферах при переключении, но не передаются для Anonymous/Certificate.
Вкладка Network хранит предыдущий протокол, поэтому при повторном открытии
восстанавливается последняя DA/UA вкладка; настройки и поля переключаемых вкладок
сохраняют прежнее поведение. Измерено Linux GUI 198 unit и 141 interaction
passed, 1 Windows-only ignored; Linux all-targets, Windows i686 workspace
all-targets typecheck с test-only флагами, fmt и CLI boundary на обоих targets
прошли. PNG, Windows runtime и финальный CI ещё не выполнены.

Тест root browse/read гонялся с ответом DA stub и нестабильно падал при
повторном прогоне. Test recorder теперь удерживает отправленный Browse pending
и проверяет единственную команду Browse и отсутствие Read; отдельный тест
прошёл, production scheduling не менялся.

**2026-10-28 — подэтап B2.** UA discovery хранит SingleHost/Network с
RequestId, Auto с непустым Host/Network/Both, отдельное ожидание подтверждения
большой подсети и активный Subnet со ScanId. Новый поиск снимает только свои
pending запросы; DA discovery отдельно отменяет только Discover, сохраняя даже
поставленный в очередь UA timeout. Ошибки и timeout отдельных шагов завершают
Auto без потери частичных результатов. ScanEnd выражает Completed, Cancelled
или Failed; для отмены и ошибки сохраняется реально полученный progress.
Headless тесты покрывают клики подтверждения и остановки, stale events, оба
направления изоляции, отказ отправки, timeout и три исхода scan. Linux backend
UA tests 62/62, contract 26/26 + 3 doctest, CLI 6/6 + live sample 2/2 прошли.
PNG, Windows runtime и финальный exact-head CI не проверены; остальные B/E/F/G/H
и baseline 10k открыты.
Измерено после финальных изменений: Linux GUI unit 204/204, UI interactions
143/143 (1 Windows-only ignored), Engine 22/22; Linux all-targets, diagnostics
bins и boundary для Linux/Windows targets прошли. Windows i686 workspace
all-targets и diagnostics bins прошли только typecheck с test-only пропуском
ресурсов/manifest; standalone backend/CLI all-targets typecheck без пропусков
прошёл. `cargo fmt --all -- --check` прошёл.

### 2026-10-28 — подэтап B3: состояния DA/CSV/Auto

Результаты DA и выбор используют `DaHit` с неизменной host identity; ручной
ProgID и fallback первого найденного сервера сохранены. CSV job различает
running, cancelled и completed; закрытие после отмены сохраняет частичный
импорт. EndpointSpec использует прямые Auto policy/mode, старые отсутствующие,
null и legacy значения принимаются; явный security не понижается. Это только
подэтап B, SessionKind и следующие состояния не затронуты.

Измерено Linux: GUI unit 204/204, UI 144/144 и 1 ignored до последних двух
добавленных DA регрессий; новые DA selection, double-click и session identity
прошли отдельно. Contract compatibility тесты 6/6, Linux GUI/CLI all-targets,
diagnostics, boundary и fmt прошли. Windows i686 workspace all-targets и
diagnostics проверены с test-only флагами, standalone backend/CLI без флагов.
После повреждения локальных metadata очищены только engine/locale target
артефакты и набор повторён. PNG renderer не собрался offline: отсутствует
`wgpu-core-deps-apple`; визуальная проверка остаётся открытой. Windows runtime,
физические DA/UAC, финальный exact-head CI, оставшиеся B/E/F/G/H и 10k baseline
не выполнены.

Повторный полный Linux набор после очистки metadata: backend 202/202,
Engine 22/22, contract 27/27 + 3 doctest, CLI 6/6 + 2/2 live sample.

CSV dialog больше не использует checked `expect`: состояние и счётчики читаются
одним snapshot через `let Some`. Linux interaction отмены и завершения 1/1,
Windows i686 GUI all-targets typecheck с test-only пропуском ресурсов и fmt
прошли; ограничения предыдущей записи сохраняются.
