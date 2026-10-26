# opcda-egui

Нативный клиент OPC на Rust + egui, переписанный с EleSy OPC Test Client 2.1.0.139
(Delphi, 2012, 32-bit, OPC DA 2.0) и расширенный поддержкой OPC UA через
протокол-нейтральный BFF (backend-for-frontend). Код и команды в этом документе —
на английском; описания и UI — на русском.

Проект находится в разработке. См. «Ограничения».

## Назначение

Клиент для работы с несколькими OPC серверами одновременно:

- **OPC DA 2.0** — через 32-битные COM-интерфейсы (`IOPCServer`,
  `IOPCBrowseServerAddressSpace`, `IOPCItemMgt`, `IOPCSyncIO`), локально и по DCOM.
- **OPC UA** — через `async-opcua` 0.19 (SecurityPolicy None, anonymous / username),
  подключение по URL вида `opc.tcp://host:4840`.

Это не web-приложение и не Electron; нет внешнего рантайма. Приложение — один
нативный `opcda-egui.exe`; для DA route steering рядом поставляется
`opcda-nethelper.exe` (запускается по UAC только при необходимости).

## Возможности

| Область | Реализовано |
|---|---|
| Подключение | DA по ProgID (локальный/удалённый), UA по URL (anonymous / username) |
| Поиск серверов | Локальное и удалённое перечисление OPC DA (OPCEnum/`IOPCServerList`) |
| Адресное пространство | Обзор веток DA и UA, корень, переходы по веткам |
| Добавление сигналов | Выбранный, дочерние сигналы, по имени (item id / NodeId) |
| Таблица сигналов | Статус, Полное имя, Записываемый тип, Тип, Значение, Качество, Время изменения, OРС сервер, Описание |
| Чтение | Периодическое (интервал настраивается), кэш DA, атрибут Value UA |
| Запись | Диалог записи с выбором типа (Auto/BOOL/I2/I4/R4/R8/BSTR/DATE и UA-типы) — в разработке |
| Проверка связи | Тест соединения, авто/ручное переподключение |
| Дерево сигналов | Загрузка/сохранение CSV |
| Журнал | Записи уровня Info/Warning/Error, сохранение/загрузка |
| Параметры | Интервалы read/test, таймаут операций, auto-reconnect, цвета, подтверждение записи, показ скрипта/журнала |
| Сохранение | Настройки, недавние серверы, порядок/видимость столбцов, текст скрипта |
| Сеть | Явный выбор адаптера (без Auto/fallback), привязка UA-трафика, LDS/mDNS, N+1 сканер |
| Сессии UA | Не более 100 одновременных UA-сессий; DA не считаются |
| DA route steering | Временный host-маршрут через `opcda-nethelper` (UAC), refcount и cleanup |

В контекстных меню дерева и таблицы доступны «Добавить в таблицу»,
«Добавить дочерние», «Развернуть/свернуть», «Изменить значение…»,
«Копировать значение», «Удалить из таблицы». Клавиши: `Delete` — удалить
выбранные, `Enter`/`F2` — изменить значение, `Ctrl+A` — выделить все.
Эти команды в текущей фазе находятся в разработке и проверяются другим
агентом.

## Архитектура

Протокол-нейтральный шлюз (`crate::bff::Gateway`) изолирует UI от конкретного
протокола. UI и хост скриптов обмениваются только `Command` и `Event`
(`src/bff/model.rs`); каждый сеанс маршрутизируется ровно на один адаптер.
Каждый адаптер владеет всеми своими ресурсами (COM/async) на собственном
рабочем потоке.

| Модуль | Назначение |
|---|---|
| `src/bff/model.rs` | `Command`/`Event`, `EndpointSpec`, `Quality`, `Value`, `ItemInfo`, `BrowseNode` |
| `src/bff/mod.rs` | `Gateway`: каналы `Command` в адаптеры, очередь `Event` назад в UI |
| `src/bff/da.rs` | COM OPC DA: подключение, обзор, add/remove, sync read, write, тест |
| `src/bff/da_variant.rs` | Конвертация COM `VARIANT`/`FILETIME`/`DATE` в нейтральные значения |
| `src/bff/ua/` | `async-opcua` 0.19: worker, discovery, session, scanner, browse/read/write/test |
| `src/bff/mdns.rs` | Нативное mDNS/DNS-SD-обнаружение OPC UA |
| `src/bff/net.rs` | Привязка UA-сокетов к выбранному адаптеру |
| `src/net.rs` | Модель адаптеров, подсетей и состояние сетевого выбора |
| `src/da_route.rs` | DA route planning, refcount, named-pipe клиент helper |
| `src/app/` | Состояние UI, сеансы, строки таблицы, диалоги, обработка событий |
| `src/ui/` | Меню, панели, дерево, таблица, диалоги |
| `nethelper/` | `opcda-nethelper.exe`: привилегированный helper host-маршрутов |
| `src/columns.rs` | Настройка столбцов таблицы |
| `src/settings.rs` | `Options` и `Settings`, путь настроек |
| `src/journal.rs` | Журнал работы (макс. 5000 записей) |
| `src/opc/bindings` | Обвязки OPC DA, сгенерированные `windows-bindgen` (см. «Лицензии») |

Биндинги COM OPC DA взяты из MIT-лицензированного `opc-da-client` 0.2.0
(см. «Лицензии и атрибуция»).

Аудит ответственности модулей и план разбиения god-object-файлов:
[`docs/audit-responsibility.md`](docs/audit-responsibility.md).
Аудит UI/UX вкладок OPC UA и «Сетевые настройки»:
[`docs/audit-ui-ux.md`](docs/audit-ui-ux.md).

Protocol-интеграционные тесты используют единый DFB-подобный seam
`tests/support/dfb.rs`: один блок `Gateway`, вход `Command`, выход `Event`.
UI-тесты остаются на `egui_kittest`.

## Сборка

Сборка **только 32-битная** (`i686-pc-windows-msvc`) — это обязательное
требование. OPC DA — 32-битная COM-технология: `OPCEnum.exe` и большинство
легаси-серверов зарегистрированы только в 32-битной ветке реестра
(`Wow6432Node`), и 64-битный процесс не сможет их инстанцировать. Оригинальный
`OPCtools.exe` тоже 32-битный. OPC UA не зависит от разрядности, но сборка
всего клиента целиком на `i686` сохраняет совместимость с легаси.

Целевая платформа зафиксирована в `.cargo/config.toml`, поэтому достаточно:

```sh
cargo build --release --bin opcda-egui --target i686-pc-windows-msvc
cargo build --release --bin opcda-nethelper --target i686-pc-windows-msvc
```

Бинари: `target\i686-pc-windows-msvc\release\opcda-egui.exe` и
`opcda-nethelper.exe` (рядом с приложением).

Упаковка обоих 32-битных exe:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1
```

Скрипт проверяет PE machine `0x014C`, копирует `opcda-egui.exe`,
`opcda-nethelper.exe`, manifest helper и README в
`target\i686-pc-windows-msvc\release\package\`.

Правила привязки, DA route steering, UAC и ручная проверка:
[`docs/network-binding.md`](docs/network-binding.md).

Требуется 32-битный тулчейн `i686-pc-windows-msvc` (MSVC). Проект использует
Rust 2024 (см. `rust-version` в Cargo.toml).

## Запуск

```sh
target\i686-pc-windows-msvc\release\opcda-egui.exe
```

UI на русском языке, тёмная тема. Для корректного отображения кириллицы
загружаются системные шрифты `segoeui.ttf` (пропорциональный) и `consola.ttf`
(моноширинный) с фолбэком.

Настройки хранятся в `%APPDATA%\opcda-egui\settings.json`. Клиентские
сертификаты OPC UA создаются автоматически в `%APPDATA%\opcda-egui\pki`.

## Настройки

`%APPDATA%\opcda-egui\settings.json` — JSON, сохраняется автоматически: опции,
порядок и видимость столбцов, текст скрипта, последние DA-хост/сервер,
последний UA-URL, до 12 недавних серверов.

| Опция | По умолчанию | Диапазон |
|---|---|---|
| Интервал обновления, с | 1 | 1..60 |
| Auto-reconnect | true | флаг |
| Цвета сигналов | true | флаг |
| Подтверждение записи | true | флаг |
| Таймаут операций, с | 60 | 1..1800 |
| Интервал тестирования связи, с | 60 | 1..1800 |
| Показывать скрипт | true | флаг |
| Показывать журнал | false | флаг |

## Тесты

- **Автономные (герметичные) модульные тесты** — по `cargo test`:
  `crate::bff::model`, `crate::columns`, `crate::journal`, `crate::settings`
  (качество/значения, настройки, столбцы, журнал).
- **Интеграционные тесты OPC UA** — в `tests/ua_smoke.rs`, помечены
  `#[ignore]`, требуют OPC UA сервер по `OPCUA_TEST_URL`
  (по умолчанию `opc.tcp://127.0.0.1:4840`): подключение, обзор корня,
  добавление `i=2259` (ServerStatus State, I64) и `i=2258` (CurrentTime,
  DateTime), чтение с качеством Good; отдельный тест — чтение несуществующего
  узла не должно приводить к панике.

```sh
cargo test
cargo test --test ua_smoke -- --ignored --nocapture
```

Подробности и статус прогонов — в `docs/test-matrix.md`.

### Живая проверка OPC UA

Рабочая конфигурация — CODESYS ControlService на `opc.tcp://127.0.0.1:4840`.
Используйте `127.0.0.1`, **не** `localhost` (тот резолвится в `::1` и
недоступен). Наблюдаемые узлы `i=2259` и `i=2258` читаются с качеством Good.
Используйте только анонимный / username вход без политики безопасности —
не публикуйте локальный сервер в сеть.

### Живая проверка OPC DA

32-битное перечисление находит `Kepware.KEPServerEX.V6` /
«KEPServerEX 6.2» (сервис `OpcEnum`). Живое подключение/чтение/запись DA
не проверены: служба `KEPServerEXV6` остановлена и не запускается
(System error 1068 — зависимость не удовлетворена; `CoCreateInstance`
завершается с `0x8007042C`).

## Ограничения

- **Интерпретатор скриптов не реализован.** В релизе работает только
  редактор и сохранение/загрузка файлов `.stx`; интерпретация не выполняется.
  Это подтверждает и UI: «Интерпретатор скриптов не реализован в этой версии».
- **Запись качества в OPC DA не реализована.** Стандартный OPC DA не имеет
  записи качества; оригинал использовал проприетарный `EleSy.OPCIntercom`,
  который не распространяется. В перезаписи эта возможность намеренно
  опущена (см. `docs/reverse-engineering.md`).
- **Запись снабжения** (write) — в разработке: диалог с выбором типа и
  преобразованием значений есть, но полное покрытие типов/сценариев ещё не
  завершено, статус по каждому типу уточняется (см. `docs/test-matrix.md`).
- **Живая проверка DA route helper не выполнена**: в текущей среде нет
  Kepware и второго DA-хоста. Код и unit-тесты готовы; ручной стенд описан в
  `docs/network-binding.md`.
- **Helper требует UAC.** Без подтверждения DA разрешён только при уже
  существующем маршруте через выбранный адаптер; иначе — явная ошибка.
- **Пиксельное визуальное соответствие оригиналу не гарантировано** — цель
  функциональная, а не воспроизведение оформления.

## Структура проекта

```
Cargo.toml            manifest, зависимости, MIT
.cargo/config.toml    принудительная 32-битная сборка i686-pc-windows-msvc
src/
  main.rs             точка входа eframe/egui
  lib.rs              публичные модули
  app.rs              состояние UI, сеансы, диалоги
  ui.rs               меню, панели, дерево, таблица
  columns.rs          настройка столбцов
  settings.rs         настройки
  journal.rs          журнал
  bff/
    model.rs          Command/Event, Quality, Value
    mod.rs            Gateway
    da.rs             COM OPC DA
    da_variant.rs     VARIANT <-> Value
    ua.rs             async-opcua
  opc/bindings/       обвязки COM OPC DA
  bin/                probe_*.rs (пробы discover/browse/read)
tests/
  ua_smoke.rs         интеграционные тесты OPC UA (ignored)
docs/
  reverse-engineering.md
  test-matrix.md
```

## Лицензии и атрибуция

Проект `opcda-egui` распространяется по **MIT** (заявлено в Cargo.toml).
Сохраняйте уведомления об авторстве:

- **opc-da-client 0.2.0 (MIT)** — исходная база COM OPC DA; обвязки
  (`src/opc/bindings/da.rs`, `comn.rs`) сгенерированы `windows-bindgen`
  и скопированы из этого проекта. Требуется атрибуция MIT.
- **async-opcua 0.19** — клиент OPC UA.
- **egui / eframe 0.37**, **egui_extras** — UI-фреймворк.
- **windows crate** — обвязки Win32/COM.
- **tokio** — асинхронный рантайм (UA-адаптер).
- **serde / serde_json** — сериализация настроек.
- **chrono** — даты/время.
- **rfd** — системные диалоги выбора файлов.

Оригинальный продукт «OPC Test Client» 2.1.0.139 принадлежит EleSy company и
используется только как источник функциональных требований.