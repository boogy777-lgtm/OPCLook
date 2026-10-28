# Статус ревью Rust

Исходный отчёт: `refactoring-plan.md`, база d85c744. Статусы ниже отражают
только проверенное в текущей ветке; старые результаты предыдущих запусков не
считаются проверкой этих изменений.

| Пункт | Статус |
|---|---|
| B1 | Подтверждён; исправлен, две регрессии Enter прошли Linux |
| B2 | Подтверждён; исправлен, Windows unit-тесты прошли CI для `2a6d017`; реальный DA не проверен |
| B3 | Подтверждён; исправлен, UTF-8 регрессия прошла Linux |
| B4 | Подтверждён; проверки UA/DA в коде, Windows unit-тесты прошли CI для `2a6d017` |
| B5–B6 | Подтверждены; исправлены, Windows unit-тесты прошли CI для `2a6d017` |
| B7 | Подтверждён; исправлен, регрессии удаления и восстановления прошли Linux |
| B8 | Подтверждён; BCrypt, взаимная проверка PID и ограниченный pipe реализованы; новые Windows pipe-тесты ожидают CI, реальный UAC/маршруты не проверены |
| B9 | Подтверждён; исправлен, регрессия скрытой колонки прошла Linux |
| B10 | Подтверждён; настройки ограничены при загрузке, регрессия прошла Linux |
| B11 | Подтверждён; SAFEARRAY guard и Windows unit-тесты прошли CI для `2a6d017`; тест освобождения вложенного элемента остаётся открытым |
| B12 | Подтверждён; guard группы при ошибке cast добавлен, COM runtime-регрессии ещё нет |
| B13 | Подтверждён; CoString проверяет null и освобождает память; Windows null-регрессия прошла CI для `2a6d017` |
| B14 | Подтверждён; восстановление poisoned mutex, регрессия отмены прошла Linux |
| B15 | Подтверждён; загрузка типов прерывается Disconnect, после timeout сессия закрывается до команд; регрессия ожидания прошла Linux, необходим live-тест |
| B16 | Подтверждён; добавлены идентификаторы завершения, Windows-регрессия прошла CI для `2a6d017` |
| B17 | Подтверждён; сохранение состояния запуска и регрессия, Linux тест пройден |
| B18 | Подтверждён; исправление и регрессия, Linux тест пройден |
| B19 | Подтверждён; Windows unit-тесты прошли CI для `2a6d017`, экстремальные границы SAFEARRAY ещё требуют проверки |
| B20 | Подтверждён; guard дескриптора и ключ маршрута исправлены, Windows unit-тесты прошли CI для `2a6d017` |
| P: RowIndex | Подтверждено: длина и проверка попадания не обнаруживают подмену строки для `has_row_for_item` |
| P: блокировки UI при подключении | GUI передаёт подготовку ограниченному сервису Engine; реальный Windows runtime ещё не проверен |

Windows runtime, COM и реальный DA пока не проверены. Остальные архитектурные,
типовые и производительные задачи отчёта ожидают последующих этапов.

Кросс-проверка типов Windows без resource compiler: только для `cargo check`
задайте `OPCLOOK_CHECK_ONLY_SKIP_RESOURCES=1`; для выпуска флаг не использовать.
Обычная сборка и Windows CI по-прежнему встраивают ресурс.

Helper unit-тесты CI собираются с `OPCLOOK_HELPER_TEST_NO_MANIFEST=1`, без
запроса UAC и без изменения маршрутов; отдельная обычная сборка после них
проверяет возврат манифеста. Не использовать флаг для выпуска.

## Передача следующему этапу

Измерено на Linux: библиотечный набор 398 passed, 0 failed, один
недетерминированный диагностический тест отфильтрован; все integration test
бинарники завершились успешно, тесты с внешними серверами и измерения оставались
ignored. Отдельно позднее прошла новая регрессия `failed_lease_drop_keeps_recovery_marker`.
Windows i686 `cargo check --tests` прошёл с указанными флагами; это проверка
типов, а не запуск, линковка ресурсов или реальный DA/UAC.

Открытые обязательства для следующих этапов:

- B8 и P UI-блокировка: `src/da_route.rs::HelperClient` выполняет синхронные
  read/write без ограниченного ожидания, DNS и helper-ожидание вызываются из UI.
  Вынести операции в платформенный сервис с ограниченным временем и отменой;
  проверить реальный UAC, несовпадающих PID и закрытие pipe на Windows.
- B12: `src/bff/da.rs::PendingServerGroup` требует проверку неудачных cast на
  COM-стенде; `CoString` внедрён в два подтверждённых места B13, остальные ручные
  освобождения COM пока не переведены на guard.
- B11: guard строительства SAFEARRAY добавлен, но регрессия ошибки вложенного
  элемента и проверка освобождения нужны на Windows; тесты массивов BOOL/R4
  и `ole_date` должны реально выполняться в CI. B19 дополнительно требует
  тест экстремальных границ SAFEARRAY.
- B15: `src/bff/ua/worker.rs` проверен имитацией зависшей загрузки, нужен
  live-тест timeout и отсутствия сетевой команды после закрытия.
- B5/B6/B16/B20: новые тесты требуют фактического Windows CI, а не только
  кросс-проверки типов. Проверить возврат повышающего манифеста после тестовой
  сборки helper. Test-only flag помечен `rerun-if-env-changed` в build.rs.
- Полный RAII-слой из раздела D отчёта ещё не закончен: `ComArray`,
  `OwnedVariant`, остальные `CoString`/`OwnedHandle` и Windows API ownership,
  комментарии безопасности и точечный lint остаются открытыми. Blanket allow
  не добавлялся. Нужны тесты освобождения при ошибках.
- Подтверждённый P `RowIndex` не исправлен: публичный `rows` допускает замену
  без изменения длины; перенести изменение за методы и добавить регрессию.

`crates/contract/` теперь отслеживается в составе переноса слоёв; прототип больше не является отдельным неотслеживаемым каталогом.

## Этап переноса слоёв — в работе

Контракт, backend, engine, локализация и CLI физически разделены; GUI и CLI
используют общий Engine. Перенесены исходники текущего этапа B1–B20 без отката
исправлений. Независимая компиляция CLI и компиляция тестовых целей Linux
проверены; также измерено: Engine 4 passed, CLI unit 2 passed, GUI integration
`ui_interactions` 138 passed и 1 ignored. Живые тесты CLI, полный гейт и
Windows runtime текущего переноса ещё не завершены. B8 и P: синхронный helper/DNS и подготовка маршрутов пока
остаются в GUI-пути, bounded/cancelable platform service ещё требуется.
Остальные пункты плана также остаются открытыми.


## Промежуточные проверки следующего этапа

GitHub Linux и Windows CI для `2a6d017` прошли все задания, включая
Windows unit-тесты; реальный COM/DA, UAC и маршруты при этом не проверены.
На Linux проверены CLI (4 теста) и Engine (5 тестов), включая ограничение
сериализованной JSON-записи, закрытый stdout и корреляцию ошибки чтения.
Из-за нулевого объектного файла, создаваемого rustc 1.99.1 для
`async-opcua-core-namespace`, тесты запущены с `codegen-units=1` только
для этого пакета.

Модели маршрута перенесены в contract, реализация в backend, локализация
остаётся адаптером GUI. Это перенос границы, не асинхронный сервис: DNS,
helper и подготовка по-прежнему вызываются из GUI. Ограниченная очередь,
отмена, завершение по дедлайну, владение lease в Engine
остаются обязательными; live CLI и UI проверки ещё не выполнены.

Helper IPC теперь использует overlapped named pipe через Tokio с лимитом
3 секунды на обмен и 16 КиБ на ответ. Ошибка обмена закрывает pipe и
инвалидирует принадлежащие helper маршруты; проверка PID сохранена. На Linux
14 route-тестов прошли, включая таймаут, лимит и инвалидирование; Windows
i686 `cargo check --tests` прошёл без запуска. Реальная проверка pipe, UAC и
удаления маршрутов в Windows остаётся открытой. DNS и UI всё ещё синхронны.

GUI теперь ставит DA discovery/connect в очередь Engine, вместо вызова DNS
и helper из кадра. Engine держит tickets до завершения discovery или закрытия
сессии, передаёт типизированные ошибки через обычный Event/accept; потеря
маршрута во время Connect и после соединения проверена отдельно. На Linux
прошли GUI lib 186, Engine 11, locale 9 и новый тест с реальными кадрами
`App::update` (по одному измеренному запуску). Windows runtime, реальный DA,
UAC и live CLI всё ещё не проверены; общий гейт текущего head не завершён.

Измерено на Linux: два subprocess live-теста CLI против sample UA прошли
(browse/read/write/read, три принудительных poll, push после 3 секунд покоя при
1-секундном timeout, Ctrl+C, секреты окружения и закрытый stdout). Тесты
ограничены Unix; Windows DA/UAC и финальный гейт текущего head остаются открыты.

Таймаут запроса теперь создаёт коррелированное типизированное событие Engine;
Engine отменяет незавершённую подготовку и посылает Disconnect после timeout
Connect. Повторный timeout не создаёт дубликат, поздний ответ отклоняется;
обработка ошибок записи остаётся в GUI. Измерено: Engine 12 и GUI lib 186
прошли на Linux; CLI использует собственный заданный deadline ожидания.

После последнего review Engine учитывает AddItems только после успешных
результатов по каждому ключу; ранний PollOnly сохраняется, ошибочный или
просроченный Add не создаёт фантомного polling, удалённый до ответа ключ не
добавляется повторно. Измерено: 16 тестов Engine и 2 live CLI-теста прошли на
Linux. Отмена после готовности helper повторно проверяется перед добавлением
маршрута; Linux-регрессия прошла. Полный Linux gate и Windows CI нового head
должны быть повторены. Реальные DA, UAC, отрицательные IPC-тесты на Windows и
остальные этапы RAII/newtype/enum/производительности остаются открытыми.

## Гейт слоя и передача

Измерено для исходников `a7d07ab`: `scripts/check-linux.sh` прошёл полностью;
backend 189, Engine 17, contract 23, locale 9, CLI unit 5 и live 2, GUI lib
186, `ui_interactions` 139 passed / 1 ignored; остальные integration-наборы
прошли с предусмотренными ignored-тестами. Использован `codegen-units=1` для
`async-opcua-core-namespace` из-за нулевого объектного файла rustc 1.99.1.
Windows i686 `cargo check --locked --workspace --all-targets` прошёл с
`OPCLOOK_CHECK_ONLY_SKIP_RESOURCES=1` и `OPCLOOK_HELPER_TEST_NO_MANIFEST=1`:
это проверка типов, а не сборка выпускных ресурсов или запуск Windows-тестов.
Финальный CI точного head на Linux и Windows ещё обязателен.

Поведенческая регрессия GUI использует реальные кадры `App::update`, но PNG
диалога не рендерился и не осматривался. Это остаётся обязательной проверкой
следующего UI/performance этапа вместе с измерением кадров и аллокаций на
реальных 10k строках; прежний `perf_rows` не является таким измерением.
Также остаются реальные DA/UAC/маршруты и Windows pipe runtime, полноценный
RAII COM/WinAPI и оставшиеся этапы newtype, enum, производительности и модулей.
При последующем изменении timeout-дренирования сохранить grace-период того же
future перед закрытием сессии; замена на отбрасывающий future timeout неверна.

## Этап D: ownership и Windows safety (промежуточный отчёт)

| Пункт | Реализация и проверка | Оставшееся |
|---|---|---|
| B11 | Guard разблокирует и уничтожает SAFEARRAY; Windows тест наблюдает первый BSTR, нулевой счётчик блокировок и успешный настоящий `SafeArrayDestroy` ровно один раз при ошибке вложенного элемента | Запуск теста на Windows CI; реальный DA |
| B12 | COM mock успешно создаёт группу, возвращает интерфейс без `IOPCItemMgt`; тест проверяет один forced `RemoveGroup` | Запуск Windows CI; реальный DA |
| B13 / CoString | Nullable COM-строки discovery, browse и ошибок обёрнуты guard; null-регрессия существует | Запуск текущего head на Windows CI |
| B19 | Длина ограничивается в i64 до преобразования в 32-bit usize; тест полного диапазона i32 и реальные небольшие массивы с крайними нижними границами | Запуск Windows CI |
| B20 / LocalAlloc | Дескриптор и SID helper освобождаются guard; ключ маршрута сохраняет next hop | Запуск Windows CI и реальный UAC/маршруты |
| COM arrays / VARIANT | `ComArray` освобождает вложенные blobs и read VARIANT, `OwnedVariant` — запись; тест счётчика cleanup при ошибке/unwind | Windows runtime и реальный DA |
| Группа / COM status | Guard группы хранится всю жизнь сессии и удаляет её ровно один раз при shutdown или drop; status и vendor освобождаются guard | Windows runtime |
| WinAPI handles / tables | Guard для helper token, SID, дескриптора, процесса и IP Helper table; тест закрытия handle и тест возврата impersonation | Windows runtime, UAC и реальные маршруты; при timeout helper завершает процесс, завершая ожидающий pipe worker |
| DNS / символы / UTF-16 | System32-only загрузка DLL с владением библиотекой; типизированные символы и общий конвертер; Windows тесты символов и Unicode | Windows runtime и сетевое обнаружение |
| Безопасные границы | `deny(clippy::undocumented_unsafe_blocks)` в `com`, `da`, `da_variant`, `winutil`, DA route, Windows inventory, сетевом binding, Windows mDNS и helper; generated COM bindings не менялись | Независимый review инвариантов и финальный Windows CI; другие этапы и проверки плана не завершены |
| Tr / diagnostics | enum и `ALL` генерируются вместе без transmute или последнего sentinel; dispatch ошибок probes проходит через Engine и `Result` | Полный gate актуального head |
| CI-исправления | Тест свежего запроса использует детерминированный pending; после ошибки helper pipe и Tokio runtime уничтожаются вместе | Реальная Windows pipe-регрессия и финальный exact-head CI |

Измерено локально: locale 9/9 и свежий K1 GUI-тест 1/1 на Linux; `cargo check`
Linux диагностических binaries с feature `diagnostics` прошёл. Windows i686
backend `--tests` и helper `--tests` прошли проверку типов и Clippy; для helper
использован test-only `OPCLOOK_HELPER_TEST_NO_MANIFEST=1`. Clippy backend тестов
запускался с командным `-A clippy::approx_constant` для ранее существовавшего
тестового литерала; это не настройка исходников и не blanket allow. Windows
тесты здесь не запускались. Rust 1.99.1 потребовал локальных codegen-units=1
для `read-fonts`, `async-opcua-core-namespace` и host build scripts из-за
повреждённого metadata/link output; изменения Cargo-конфигурации не коммитились.
Все прежние числа Linux gate относятся к обозначенному старому head, не к
этому этапу. Рендер PNG, 10k frame/allocation benchmark, реальные DA/UAC,
pipe runtime, новые типы/enum, производительность и разделение модулей остаются
открытыми. При изменении UA timeout сохранять grace-дренирование того же future.

После описанной матрицы проверка формата DA VARIANT дополнена: BYREF,
неизвестные типы элементов, неверный размер и глубина свыше 32 возвращают
`Value::Other`; это явный отказ, не поддержка этих форматов. Заимствованный
SAFEARRAY разблокируется guard даже при unwind. Windows регрессии на эти
случаи прошли только typecheck и ожидают выполнения CI.

Повторная Linux проверка backend lib на текущих исходниках: измерено 189 passed,
0 failed, 0 ignored (60,42 с); текущий Windows runtime и полный exact-head CI
ещё не выполнены. Helper явно сообщает об ошибке запуска потока и об ошибке
возврата impersonation; guard остаётся резервом при раннем выходе.

`SafeArrayBuilder::finish` теперь передаёт массив только после успешного
Unaccess/Unlock; неудача возвращает типизированную ошибку, Drop повторяет
освобождение и вызывает Destroy. Тесты с одноразовым отказом каждой операции
проверяют повтор и успешное настоящее уничтожение; пока выполнен только
Windows typecheck. При постоянном сбое OLE освобождение не гарантируется.
Helper после timeout завершает процесс, поэтому ожидающий поток ConnectNamedPipe
и его клонированный handle ограничены временем жизни этого процесса.

## Тип фильтра Browse (этап C, частично)

`CommandKind::Browse` и состояние фильтра сессии используют `NodeClassFilter`
вместо неявных чисел и отдельного bool. `Variables` по-прежнему включает UA
Object/folder для навигации. Это только один пункт этапов A/C; идентификаторы,
качество строк, настройки и остальные типизированные границы ещё не завершены.
Полный Linux gate, Windows runtime и exact-head CI остаются обязательными.
Локально прошли Linux all-targets typecheck для GUI/CLI и библиотек и один
backend-тест маски (1/1), а также Windows i686 workspace all-targets typecheck
с test-only отключением ресурсов/манифеста; Windows тесты не запускались.

Синтетический корень Views имеет одну константу и метод `BrowseNode`; проверка
идентификатора прошла в contract (1/1 Linux), GUI all-targets typecheck прошёл
на Windows i686 с test-only отключением ресурсов. Остальные пункты A/C открыты.

Старые settings с отсутствующими полями `Settings` и `Options` дополняются
значениями по умолчанию и проходят прежний clamp при загрузке; имеющиеся поля
сохраняются. Новый тест загрузки частичного файла прошёл Linux (1/1), Windows
GUI all-targets typecheck с test-only отключением ресурсов прошёл. Остальные
изменения настроек и проверка Windows runtime открыты. Дополнительно Linux
U5 app unit test переключения фильтра прошёл (1/1).

`column_order` теперь хранит `Vec<ColumnId>`; старые quoted/plain имена
разбираются на границе, дубликаты и неизвестные имена отбрасываются, недостающие
дополняются. При отсутствии распознанных имён сохранён прежний fallback на
`columns.order`. Файл по-прежнему записывает прежние plain имена. Linux settings
46/46 и UI interaction 1/1 прошли; Windows GUI all-targets typecheck с test-only
отключением ресурсов прошёл, Windows runtime и остальные пункты C остаются.

UI RowQuality отделяет реальные коды сервера от NoData/Waiting и локальной
ошибки. Контрактный DataValue и CLI по-прежнему хранят Quality сервера;
регрессия проверяет оба прежних sentinel-кода как настоящие серверные коды.
Linux: `ui_polish_table` 17/17, отбор `stale` в ui_interactions 5/5, CLI unit
5/5; Windows i686 workspace all-targets typecheck с test-only флагами прошёл.
PNG не рендерился; полный gate, Windows runtime и остальные A/C открыты.

Язык в Settings теперь `Lang`: legacy варианты читаются при загрузке, неизвестный
код и отсутствие поля дают русский, запись сохраняет коды ru/en/zh. Linux
settings 46/46, взаимодействие меню языка 1/1 и Windows GUI all-targets
 typecheck с test-only отключением ресурсов прошли. Остальные A/C открыты.

Интервал refresh DA-маршрута централизован как Duration в backend и реально
используется сервисом; compile-time assert проверяет refresh < lifetime и UA
lifetime count >= 3 × keepalive. Wire lifetime остаётся u32 секундами. Linux
тест констант 1/1, Windows i686 workspace all-targets typecheck с test-only
флагами и fmt прошли; Windows runtime и остальная типизация времени открыты.

Settings хранит выбранный AdapterId и IpAddr в типизированном виде. Старые
строковые поля читаются при загрузке, неверные значения становятся unselected,
валидные записываются канонически; stale GUID по-прежнему сохраняется. Linux
settings 46/46 и network-tab interactions 5 passed / 1 Windows-only ignored;
Windows GUI all-targets typecheck с test-only отключением ресурсов прошёл.
Реальный Windows inventory/маршруты и остальные пункты A/C открыты.

VarType с именованными VT-константами проходит из GUI через WriteRequest к DA;
неизвестные/модифицированные u16 сохраняются, UA type_id не смешивается с VT.
Linux backend VT test 1/1, GUI K3 interaction 1/1 и all-targets check прошли;
Windows workspace all-targets typecheck с test-only флагами прошёл без runtime.
На Windows фактические VARIANT и полный gate ещё требуют CI.

Общий `UaUrl` введён частично: GUI connect/discover, CLI и backend connect/TCP
используют проверку схемы, host, IPv6 и порта; userinfo, port 0 и управляющие
символы отклоняются до записи GUI в журнал. Сравнение host учитывает регистр
DNS и нормализует IP, путь сохраняет регистр, query/fragment сравниваются без
снятия завершающих slash. API сравнения пока не подключён ко всем путям
обнаружения и объединения: это обязательный следующий шаг. В используемом SDK
`hostname_port_from_url` опирается на `url::Url`, который не принимает IPv6 zone
identifiers; поддержка scoped URL здесь не заявлена. Typed advertised security,
NonZero индексы и role ID также остаются открытыми; Windows runtime и финальный
exact-head CI обязательны.
Локально прошли contract URL 1/1, GUI userinfo-before-journal 1/1 и U11
host/loopback 3/3 на Linux; fmt, Windows i686 backend/CLI all-targets typecheck
без resource skip и GUI all-targets typecheck с test-only отключением ресурсов.
Для Linux сборки локально использован codegen-units=1 также для `vello_cpu`
после zero-length object rustc; этот workaround не меняет repository. Windows
runtime не запускался; проверка прежнего exact head выявила heap corruption,
которая требует отдельного исправления до final CI.


Windows CI run 40678175913 изолировал heap corruption в тесте смешанных
BYREF/неверных layout; остальные изолированные COM/VARIANT тесты прошли.
Причина: `windows` 0.61 реализует `Drop` для `VARIANT` в extension-модуле,
вызывая `VariantClear`. Тестовый невалидный VARIANT и ручной Destroy двойным
образом уничтожали массив; заимствованный временный VARIANT при чтении BSTR
массива также мог освобождать строку владельца. Не владеющие представления
переведены в `ManuallyDrop`, владеющий guard уничтожает массив один раз;
`OwnedVariant` полагается на штатный Drop. Добавлен тест трёх повторных чтений
BSTR и уничтожения массива. Измерено локально: fmt и Windows i686 backend
`--tests` Clippy typecheck прошли; Windows runtime после исправления и
финальный exact-head CI ещё не прошли.

UA URL identity хранит распарсенный IP и сравнивает DNS без повторной
нормализации; дерево и сканер группируют по типизированной identity, путь и
query сохраняют регистр. LDS ввод использует общий parser; loopback alias
дедуплицируется по порту, поэтому пользовательский LDS порт не теряется.
Linux discovery 12/12, scanner 2/2, U15 8/8, UI grouping 2/2; Windows i686
workspace all-targets typecheck с test-only флагами и fmt прошли. Некоторые
циклы merge ещё повторно вызывают parser для строк: перенос parse-once всех
путей остаётся открытым, как и типизация advertised security/NonZero индексов,
Windows runtime и полный gate.

`UaEndpointInfo.url` и `UaNetworkInfo.discovery_urls` теперь проверенные UaUrl;
UI, scanner и discovery сравнивают сохранённую identity без повторного разбора.
Некорректный advertised URL отклоняется с локализованной диагностикой без
вывода адреса; остальные корректные результаты остаются доступны. Сессии
сравнивают typed URL identity, policy и mode. Linux discovery 13/13, scanner
2/2, session identity 2/2, UI grouping 2/2, security-filter 10/10, locale 9/9;
Windows i686 backend/CLI standalone all-targets typecheck без skip и workspace
all-targets typecheck с test-only флагами прошли. Live Windows runtime и полный
gate остаются открытыми. Typed advertised security и NonZero индексы ещё открыты.

URL ключ сессии теперь enum Valid/Invalid, исключающий противоречивые поля;
Linux session identity 2/2 и Windows GUI all-targets typecheck с test-only
resource skip прошли. Runtime и остальные пункты C/A открыты.

FILETIME DA timestamps теперь вычитают смещение 1601→1970, сохраняя 100-нс
доли и zero sentinel. Общий checked helper проверен Linux тестом (epoch,
доля, тик до epoch, ноль и u64::MAX: 1/1); Windows wrapper тест с настоящим
FILETIME прошёл только i686 backend `--tests` typecheck без resource skip и
ожидает Windows runtime. Реальный DA и итоговый exact-head CI остаются открыты.

Advertised security теперь `Unspecified/Known/Unknown`: неизвестные wire
значения сохраняются, не становятся None/Auto, а SDK mode сопоставляется
напрямую, без Debug-string roundtrip. Неизвестный namespace с `#None` не
считается стандартной политикой. Scanner сохраняет разные пары security одного
URL. Linux backend UA 62/62, GUI security 10/10, live CLI sample 2/2;
Windows workspace all-targets typecheck с test-only флагами и contract check
на Rust 1.95 прошли. Для Linux локально добавлен codegen-units=1 для engine
после повреждённого metadata; это не настройка репозитория. Windows runtime,
NonZero индексы, role IDs и полный gate остаются открыты.

Индексы IPv4/IPv6 и scope хранятся `Option<NonZeroU32>` от inventory/CLI до
contract NetworkBinding. Legacy cached adapter JSON читает числовой 0 как None
и пишет отсутствующий interface как прежний 0; ненулевые индексы и выбранный
адаптер сохраняются. Для link-local с нулевым/отсутствующим scope используется
IPv6 interface index; явный scope сохраняет приоритет. Linux network 35/35,
CLI unit 6/6, settings 46/46, network-tab 5 passed / 1 Windows-only ignored;
Windows workspace all-targets typecheck с test-only флагами, Rust 1.95 contract
check и boundary script прошли. GUI test потребовал локального CARGO_INCREMENTAL=0
после повреждённого engine rlib; изменения конфигурации не коммитились.
Реальный Windows inventory/DA и final exact-head CI ещё обязательны.

Фиксированные пределы reconnect и timeout Engine выражены Duration;
backoff использует bounded checked doubling и сохраняет прежнюю последовательность
1,2,4,8,16,32,60 секунд. Настройки seconds/ms и DA helper wire seconds остаются
числовыми ради совместимой схемы; WinAPI миллисекунды преобразуются проверенно.
Linux Engine 18/18 и GUI reconnect 3/3, Windows workspace all-targets typecheck
с test-only флагами и fmt прошли. Финальные Linux/Windows exact-head CI,
реальный DA/UAC и остальные этапы ещё обязательны.

Review расширил границу bounded backoff до 96 удвоений: положительная Duration
не меньше 1 нс, Duration::MAX меньше 2^94 нс; нулевая база возвращается сразу.
Linux регрессия для 1 нс/Duration::MAX/u32::MAX и нулевой базы 1/1, Windows
engine all-targets typecheck без resource skip и fmt прошли. Финальный CI нужен.

`UaUrl` дополнительно отклоняет запрещённые opaque-host символы SDK и знак в
порту до журналирования. Linux сравнение с реальным SDK и проверка отсутствия
secret в ошибке 1/1, contract URL 1/1, Windows backend `--tests` typecheck без
resource skip и fmt прошли. Остальные непройденные этапы и финальный CI открыты.

## Передача этапа C (2026-10-28)

| Пункт | Состояние | Проверка и ограничения |
|---|---|---|
| Порядок колонок | `Option<Vec<ColumnId>>`, legacy quoted/plain на входе, dedup/fill и прежний plain JSON | Linux settings 46/46 и UI interaction ранее прошли; итоговый CI нового head ещё нужен |
| Язык / сеть settings | `Lang`, `AdapterId`, `IpAddr`; старые строки, unknown language fallback, stale GUID и формат сохранены | Linux settings и network UI ранее прошли; реальный Windows inventory отдельно не проверен |
| Advertised security | `Unspecified/Known/Unknown`, SDK mode mapping напрямую; неизвестные значения и чужой `#None` не понижаются, scanner сохраняет варианты одного URL | Linux UA 62/62, GUI security 10/10 и live CLI 2/2 ранее прошли. Публичный `Known(Auto)` ещё можно создать вручную, но resolver/UI отклоняют его: регрессии ниже; дальнейшее исключение такого состояния относится к B |
| Defaults / clamp | `#[serde(default)]` и clamp при загрузке, значения известных полей сохраняются | Linux legacy-file regression прошёл; повреждённые security значения по-прежнему диагностируются |
| UA URL | `UaUrl` проверяется на входе; discovery/scanner хранят типизированные URL и IP identity, сравнивают host без учёта регистра и path/query с учётом регистра; userinfo не попадает в диагностику | Linux discovery, scanner, session identity и live CLI ранее прошли; IPv6 zone URL не поддерживает используемый SDK. Сохраняемые старые endpoint strings разбираются при connect для совместимости |
| Время | Runtime reconnect/request floors — `Duration`, bounded checked backoff, compile-time UA lifetime и route refresh инварианты; settings/wire числа сохранены | Engine 18/18 и GUI reconnect 3/3 ранее прошли; WinAPI ожидание проверенно преобразует миллисекунды |
| Связанные A | RowQuality, Views root, VarType и NonZero interface/scope выполнены в отдельных checkpoints | Windows runtime этих изменений требует финального CI; SessionId/RequestId/ItemKey/ScanId, Origin/Owner и checked allocators передаются следующему этапу |

Ручной `Advertised::Known(Auto)` отклоняется в resolver и UI; Linux backend
регрессия 1/1, UI регрессия 1/1, Windows i686 workspace all-targets typecheck
с test-only флагами прошли. CI run 40679654282 (remote `557fca20`) зелёный на
Linux/Windows и подтвердил D heap-corruption hotfix, но не содержит последующие
изменения C и FILETIME и не является финальным exact-head CI.

Передача: роли/корреляция/checked allocators (A), недопустимые комбинации и
состояния (B), типизированные ошибки с сохранением grace-дренирования того же
future (E), измеренный 10k frame/allocation baseline и производительность
(F/G), модули/Clippy и итоговая консолидация документации (H) остаются открыты.
Также обязательны точный финальный Linux/Windows CI, реальный DA/UAC/маршруты и
визуальная проверка ранее указанного диалога. Старые результаты не переносятся
на новый head; работоспособность реального оборудования не заявляется.

## Этап A: ScanId (частично)

| Пункт | Состояние | Проверка и ограничения |
|---|---|---|
| ScanId | Ненулевой отдельный тип от GUI до scanner; checked выдача последнего значения ровно один раз, после исчерпания локализованная ошибка в UI и журнале | Linux exhaustion 1/1, scanner 10/10, stale UI 1/1; Windows i686 workspace all-targets typecheck с test-only флагами. Runtime и финальный exact-head CI ещё нужны |
| SessionId и Owner | Открыто | SessionId остаётся числовым alias, discovery использует session 0; RequestId, ItemKey и Origin выполнены в последующих checkpoints |

PNG проверка UI и физические DA/UAC/маршруты остаются открытыми.

Счётчики GUI сессий и сигналов теперь checked: максимум выдаётся один раз, затем видимая локализованная ошибка; частичная партия сохраняется и отправляется. Linux регрессии 2/2, Windows i686 workspace all-targets typecheck с test-only флагами; сами SessionId и ItemKey пока числовые алиасы, а Origin/Owner и Windows runtime ещё открыты.

Request allocator уже использовал checked successor; теперь exhaustion проверяется до изменения ожидающих запросов и отмены маршрутов, а DA подготовка возвращает типизированную локализованную ошибку до старта сервиса. Linux Engine 19/19, Windows i686 workspace all-targets typecheck с test-only флагами. Ролевая типизация и удаление sentinel открыты.

ItemKey типизирован ненулевым приватным NonZeroU32; проверенная выдача сохраняет прежнее поведение исчерпания. UA и DA делают явные преобразования в wire handle, UA callback отклоняет zero и пишет локализованную диагностику; compile-fail проверяет несовместимость ItemKey/ScanId. Linux GUI exhaustion 1/1, headless удаление 1/1, contract compile-fail 1/1; Linux all-targets и Windows i686 workspace all-targets typecheck (test-only флаги) прошли. SessionId/RequestId и Origin/Owner ещё открыты, физический DA/UAC и Windows runtime не проверены.

Измерено после ItemKey: Linux backend 199/199, Engine 19/19, CLI 6/6; Windows backend/CLI standalone all-targets typecheck без resource skip прошёл. Первый backend run выявил тестовый fallback, вычислявший недопустимый zero; исправлен и полный набор повторно прошёл.

**2026-10-28 — A, RequestId и Origin:** RequestId — приватный ненулевой тип с checked successor; Event различает Reply и Unsolicited, Command несёт Option<RequestId>. Engine и DA/UA сохраняют корреляцию ответов, таймаут использует тот же токен, поздний ответ отбрасывается; внутренние Disconnect не расходуют ID. UI обрабатывает отказ отправки без ложного ожидания browse, search, scan, add, resolve, reconnect и записи; частичная запись корректно завершает ранее принятую часть при закрытии worker. Измерено на Linux: backend 200/200, CLI 6/6 и live 2/2, contract 25/25 и doctest 2/2, locale 9/9, Engine 20/20; GUI unit 196/196 и headless UI interactions 139/139 (1 ignored). Windows i686 workspace all-targets — только typecheck с test-only пропуском ресурсов, standalone backend/CLI — без пропуска; Linux и Windows diagnostics bins отдельно typechecked, Windows с test-only пропуском ресурсов; Windows runtime/DA/UAC и exact-head CI не проверены. SessionId/Owner (включая session 0 discovery), B/E/F/G/H, PNG и 10k baseline остаются открыты. В locale sample добавлен пропущенный InvalidItemHandle, обнаруженный Linux CI предыдущего checkpoint; проверка полноты не ослаблена.

**2026-10-28 — A завершён (SessionId/Owner):** SessionId — отдельный приватный ненулевой тип с проверенной выдачей; Owner явно различает Discovery и Session. В контракте Command/Event, DA/UA, Engine, GUI, CLI, probes и fixtures отсутствует sentinel session 0. Pending и таймауты ключуются Owner и RequestId; сессионные runtime и маршруты используют SessionId. DA/UA отклоняют несовместимого владельца. Регрессии проверяют разделение discovery и session, owner таймаута, корректное исчерпание, а также ровно один реальный untracked Disconnect через test recorder при таймауте Connect. Unsolicited Values/Read Error больше не снимают индикатор ожидающего Read в GUI. Измерено Linux: backend 202/202, contract 26/26 + 3 doctests, locale 9/9, CLI 6/6 + live 2/2, Engine 22/22, GUI unit 198/198, headless UI interactions 139/139 (1 ignored). Windows i686 workspace all-targets typecheck с test-only флагами, standalone backend/CLI без пропуска GUI ресурсов, Linux и Windows diagnostics bins typecheck. Это не проверка Windows runtime или физического DA/UAC; exact-head CI, PNG и измеренный 10k baseline остаются обязательными. Темы B/E/F/G/H остаются открытыми; завершение A не означает завершения плана.

## Этап B: состояния диалогов (частично, 2026-10-28)

| Состояние | Изменение и проверка |
|---|---|
| Confirm | `Option<ConfirmRequest>` с обязательным действием; Yes забирает запрос, No/Escape освобождает. Enter открытия и модифицированный Enter не подтверждают запись; headless регрессии прошли |
| UA identity | `Anonymous/UserName/Certificate`; неактивные поля сохраняются при переключении, но в endpoint попадает только выбранная авторизация. Headless переключение и certificate endpoint проверены; локализации и certificate settings сохранены |
| Connect pane | `Da/Ua/Network { previous }`: сетевой экран хранит предыдущий протокол без отдельного противоречивого поля. Headless переключение, сохранение полей и reopening проверены; search и настройки покрыты существующим набором |

Измерено на Linux: GUI unit 198/198, UI interaction 141/141 (1 Windows-only
ignored), all-targets typecheck и boundary script (Linux и Windows targets),
fmt. Windows i686 workspace all-targets прошёл typecheck с test-only пропуском
ресурсов и helper manifest, без выполнения. PNG рендер не проверен: доступного
renderer feature пока нет. Остальные состояния B, E/F/G/H, 10k baseline,
Windows runtime, реальный DA/UAC и финальный exact-head CI остаются открыты.

В повторных прогонах `read_tick_is_suppressed_while_root_loading` выявлена
гонка ответа DA stub: теперь test recorder удерживает реальный Browse pending
и проверяет ровно один Browse без Read. Production scheduling не изменён.

## Подэтап B2: UA discovery и завершение сканирования (2026-10-28)

| Пункт | Состояние и проверка |
|---|---|
| UaSearch | Idle, SingleHost, Network, Auto с непустым Host/Network/Both, ConfirmSubnet и Subnet с ненулевым ScanId. Результаты и последний progress/error остаются отдельно. Только коррелированный ответ снимает соответствующий запрос; частичный отказ и timeout освобождают состояние |
| Изоляция | Новые DA и UA поиски снимают только свои запросы; регрессии проверяют оба направления, включая уже поставленный в очередь UA timeout и устаревший ответ |
| ScanEnd | Completed/Cancelled/Failed с отдельным found исключают противоречивые исходы; частичные результаты и фактический progress сохраняются при отмене/ошибке. Проверены stale ScanId и UI подтверждения большой подсети до отправки Scan |

Измерено: Linux GUI unit и headless UI, Engine, backend UA scanner/tests,
contract, CLI unit и live sample; точные количества записаны в devlog. Windows
workspace i686 — только typecheck с test-only пропуском ресурсов и manifest,
standalone backend/CLI — typecheck без этих флагов. PNG, Windows runtime,
физические DA/UAC и финальный exact-head CI остаются открыты. Это лишь подэтап
B2: остальные состояния B, E/F/G/H и измеренный 10k frame/allocation baseline
не завершены.

## Подэтап B3: DA, импорт CSV и Auto security (2026-10-28)

| Состояние | Изменение и проверка |
|---|---|
| DA discovery | Результат и выбранный результат хранят `DaHit { server, host }`; merge различает одинаковые ProgID на разных компьютерах, первый результат остаётся fallback. Headless тесты проверяют выбор и двойной клик по нужному host, существующие проверки — manual target и изоляцию маршрутов |
| CSV import | `Running/Cancelled/Completed`, отсутствие job означает idle. Отмена сохраняет частично загруженные строки, close освобождает job; headless тест проверяет частичную отмену и следующий завершённый импорт |
| Endpoint security | Policy и mode напрямую содержат Auto; отсутствующие/null/legacy значения читаются, Auto не записывается, явные значения сохраняются. Контракт и session identity проверяют совместимость; прежние UI и backend проверки некорректного security остаются |

Измерено на Linux: GUI unit 204/204 и UI 144/144 (1 Windows-only ignored),
затем отдельно новые identity, DA selection и double-click регрессии прошли;
GUI/CLI all-targets, diagnostics и boundary script прошли. Windows i686
workspace all-targets и diagnostics прошли только typecheck с test-only
пропуском ресурсов/manifest; standalone backend/CLI — без пропусков. PNG не
проверен: временный renderer требует отсутствующий в offline cache пакет
`wgpu-core-deps-apple`. Windows runtime, физические DA/UAC, exact-head CI,
остальные состояния B, E/F/G/H и измеренный 10k baseline остаются открыты.

Повторный Linux набор после очистки повреждённых metadata: backend 202/202,
Engine 22/22, contract 27/27 + 3 doctest, CLI 6/6 + 2/2 live sample.

CSV dialog читает snapshot без checked `expect`; Linux interaction отмены и
завершения 1/1 и Windows i686 GUI all-targets typecheck с test-only пропуском
ресурсов прошли. Предыдущие ограничения сохраняются.

## Подэтап B4: тип состояния сессии (2026-10-28)

`SessionKind::Da` и `SessionKind::Ua(UaSessionView)` отделяют выбранный View,
фильтр Browse, запрошенный timeout и состояние подписки от общего дерева и
планирования. Protocol определяется видом сессии; endpoint закрыт для прямой
мутации, согласованное обновление сохраняет UA View и фильтр при том же
протоколе и создаёт новое UA состояние при смене протокола. Engine остаётся
источником состояния чтения и подписки; unsolicited Values и Read Error не
снимают ожидающий Read. Существуют headless проверки DA/UA управления, опроса,
подписки, reconnect и endpoint dedup; добавлены проверки переходов типа,
отображения UA-полей, изменения фильтра и сохранения View/filter и timeout при
reconnect.

Измерено на Linux: GUI unit 206/206, UI interaction 148/148 (1 Windows-only
ignored), Linux all-targets typecheck с test-support; fmt. Windows i686
workspace all-targets — только typecheck с test-only пропуском ресурсов и
manifest. PNG не проверен, Windows runtime, физические DA/UAC и финальный
exact-head CI остаются открыты. Это только SessionKind: helper/IPC/journal и
прочие B/E/F/G/H, а также 10k frame/allocation baseline ещё не завершены.
