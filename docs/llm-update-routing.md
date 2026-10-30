# Входной документ для LLM: маршруты обновления OPCLook

Дата: 2026-10-30. ID: R2.ROUTE. Область: получение значений OPC UA/DA.
База исследования: `main@4738cbd9ace14bdd3f59808f40ffa7f8c635a2f0`.
Это входное задание для нового чата. Маршруты направляют к существующим
механизмам; они не означают, что runtime уже обновлён.

## 1. Начало работы

1. Прочитай [AGENTS](../AGENTS.md), [status](status.md) и актуальный HEAD.
2. Прочитай [обязательную native-миграцию](native-backend-migration.md) и
   [карту владельцев библиотек](backend-ownership.md).
   Начни с R2.0 — use/configure/adapt audit. Не начинай с переписывания SDK.
3. По запросу пользователя выбери все подходящие route IDs из таблицы ниже.
   Новая ошибка одного класса идёт в существующий маршрут, а не в новый сервис.
4. Открой документы и реальные символы маршрута. Пути относятся к базе;
   если символ перенесён, найди через rg и обнови карту.
5. До кода запиши Class/Inv/N+1/Defect с местами и проверкой по правилам 3.1.
6. Сначала воспроизведи контрактный случай/пробел библиотеки. Затем минимальное
   use/configure/adapt или замена native SDK, содержательные тесты и актуализация документов.
7. Подготовь проверенный коммит; сохрани честные ограничения Windows/DA/live.

Не повторяй код примеров как тест oracle. Тесты должны вызывать настоящий
Engine/adapter entry point и наблюдать команды, state и terminal outcomes.

## 2. Документы по назначению

| Документ | Роль |
| --- | --- |
| [native-backend-migration](native-backend-migration.md) | Прямое требование владельца: native SDK для UA/DA; смена кандидата разрешена; собственный OPC data-plane исключён |
| [backend-ownership](backend-ownership.md) | Кто уже реализует механизм, какие library gaps надо доказать |
| [opc-foundation-gap-review](opc-foundation-gap-review.md) | Источники F01–F14, находки OF01–OF12, нормативное/проектное/OPEN |
| [connection-acquisition](connection-acquisition.md) | Контракт AcquisitionConfig и принадлежность соединению |
| [value-refresh-plan](value-refresh-plan.md) | Этапы R2.0–R2.6 и общие инварианты |
| [acquisition-truth-tables](testing/acquisition-truth-tables.md) | Ожидаемые результаты G/D/U/C/E/S/T/M/A/O |
| [read gate CSV](testing/acquisition-read-gate.csv), [demand CSV](testing/acquisition-demand.csv) | Полные булевы комбинации G/D, а не отчёт об исполнении |
| [devlog](devlog.md), [audit](audit-2026-10-27.md) | Измеренные результаты, открытые вопросы и история |

Routing — навигация, а не новый источник протокольной семантики.
При противоречии сверяй первичный источник и актуальный код, затем обновляй
решение и связанные тесты вместе. Проектное ограничение обозначай P;
не называй ограничение UI Kepware запретом OPC Foundation.
Не переносить исторический зелёный CI на новый HEAD.

## 3. Routing table

| Route | Триггер запроса | Ответственный механизм и invariant | Сначала читать | Основные места кода | Приёмка |
| --- | --- | --- | --- | --- | --- |
| R00-REUSE | «костыль», «библиотека уже умеет», новый backend/recovery | Native SDK обязателен; один владелец каждого lifecycle | native-backend-migration; backend-ownership; OF05/OF12 | crates/backend/Cargo.toml; Cargo.lock; ua/session/connection.rs; bff/da.rs | O28/O30–O35; trace владельцев |
| R01-CONFIG | Poll/Subscription, интервал, per-connection настройки, CLI | AcquisitionConfig snapshot; capability без скрытой смены режима | connection-acquisition | crates/contract/src/model/endpoint.rs; src/settings.rs; src/ui/dialogs/connect.rs; crates/cli/src/main.rs | C01–C26; U01–U09; M01–M12 |
| R02-DEMAND | Пустой тег после Add; старое значение после Write; Refresh | Существующий Engine queue/read-slot; pending отдельно от in-flight | value-refresh-plan; таблицы G/D/E | crates/engine/src/lib.rs; src/app/events.rs; src/app/runtime.rs | Полные CSV G/D; E01–E22; S01–S07 |
| R03-IDENTITY | Late reply/push, Remove/re-add, смена режима | RequestId + Connect epoch + registration guard; чужое событие не меняет state | OF12; identity раздел таблиц | crates/engine/src/lib.rs; crates/contract/src/model/ids.rs; ua/worker.rs; bff/da_router.rs | S08–S10; S13–S16; O12–O13/O27 |
| R04-UA-PARAMS | Фактический publish/sample/queue/keepalive, requested vs revised | SDK negotiated state -> adapter -> contract; не подменять requested | OF01/OF04; F05 | ua/session/subscription.rs; ua/session/mod.rs; crates/backend/src/constants.rs; contract/event.rs; contract/endpoint.rs | O01–O02; O08–O09 |
| R05-UA-FILTER | Timestamp-only update, deadband, изменение quality | DataChangeTrigger/Filter адаптера; качество не скрыто фильтром | OF02; F03/F11 | ua/values.rs; ua/session/mod.rs; ua/session/subscription.rs | O03–O05; T01–T17 |
| R06-UA-QUEUE | Потеря переходов, Overflow, обещание полной истории | SDK monitored queue; latest-view и history completeness раздельны | OF03; F04 | ua/session/mod.rs; ua/values.rs; contract/quality.rs; contract/value.rs; src/ui/status.rs | O06–O07; A03 |
| R07-UA-RECOVERY | Разрыв связи, missing sequence, Republish/Transfer/ResendData | SDK recovery owner; gap остаётся виден; Engine не второй Publish loop | R00; OF05/OF12; F01/F02/F14 | ua/session/connection.rs; ua/session/subscription.rs; ua/worker.rs; sdk locked source | O10–O13; O27–O28 |
| R08-WRITE | Ok без нового значения, Uncertain, async completion, timeout | Raw per-item write outcome + read-back intent; unknown не success | OF06; F06/F07 | ua/io.rs; bff/da.rs; contract/event.rs; contract/message.rs; src/app/events.rs | O14–O17; C23–C24; S17–S18 |
| R09-DA-METHOD | DA Sync/Async, Cache/Device, Subscription + Sync | DA library/adapter strategy; method отдельно от UpdateMode | R00; OF10/OF11; F12/F13 | bff/da.rs; bff/com.rs; opc/bindings/da.rs; bff/da_variant.rs | C03–C07; O23–O26; A04–A10 |
| R10-LIMITS | Большая пачка, TooManyOperations, partial reply | Existing bounded/adaptive chunker; key outcomes сохранены | OF07; F08 | ua/session/core.rs; ua/session/mod.rs; ua/session/subscription.rs; ua/io.rs | O18–O19; E11/E19/E20 |
| R11-ALIASES | RegisterNodes, NodeId alias, reconnect reuse | SDK/adapter scoped aliases; canonical identity отдельно | OF08; F09 | ua/session/items.rs; ua/session/core.rs; contract/ids.rs; contract/endpoint.rs | O20–O21 |
| R12-INITIAL | Early push, AddItems success без Good data, grace timeout | Registration outcome и snapshot outcome различны | OF09; F10; R02 | ua/session/items.rs; ua/session/subscription.rs; crates/engine/src/lib.rs | O22; E01–E03 |
| R13-PRESENT | Время/quality/source в строке, ввод в заблуждение | Common result contract + UI; received_at не server time | OF02/OF06; F11 | contract/value.rs; contract/quality.rs; contract/event.rs; ua/values.rs; src/app/events.rs; src/ui/table/cells.rs; crates/locale/src/lib.rs | T01–T17; O03–O07/O14 |

Пути с префиксом `ua/`, `bff/`, `contract` сокращены только в таблице:
ua = crates/backend/src/bff/ua; bff = crates/backend/src/bff;
contract = crates/contract/src/model; opc = crates/backend/src/opc. Точные файлы проверять перед изменением.
Generated bindings не править вместо безопасного facade/adapter;
изменение ABI требует отдельного обоснования и Windows-проверок.

## 4. Порядок реализации без дублирования

| Этап | Вход | Выход |
| --- | --- | --- |
| R2.0 | R00 и реальные library APIs/source/trace | Owner matrix и доказанный недостающий invariant |
| R2.1 | R02/R03 и G/D/E/S | Один Engine demand/read path |
| R2.2 | Успешный R2.1 | GUI/CLI wiring без собственных add/write refresh-очередей |
| R2.3 | R01/R09 | Общий конфиг + тонкий adapter выбранного native SDK; без собственного OPC COM слоя |
| R2.4 | R04/R07/R10/R12 | Negotiation, fairness, initial/recovery снимок с owner SDK |
| R2.5 | R05/R06/R08/R13 | Метки/качество/raw statuses/provenance без потери смысла |
| R2.6 | Все выбранные маршруты | Реальные тесты, отчёт точного HEAD и ограничения |

Этапы не требуют полной реализации всех функций OPC UA/DA.
Republish/Transfer/COM lifetime проверяются у владельца; в Engine остаются
только нормализованные факты и продуктовые гарантии. DA кандидат можно менять без повторного подтверждения выбора владельцем;
сначала сравнить coverage/типы/статусы/x86/lifetime на нашем контракте.
При пробеле SDK не реализовывать его OPC функции внутри приложения.

## 5. Минимальный выход агента

До изменений:
`Task -> routes -> facts/sources -> library owner -> missing invariant ->
affected files -> tests`.

После изменений:
`changed behavior -> route/test IDs -> measured results -> unverified ->
removed duplicate paths -> commit/CI`.

Условия полноты:
- Новая same-class потребность не добавляет второй sender/scheduler/recovery.
- API библиотеки использовано или документирован воспроизводимый пробел.
- Сохраняются исходные данные и диагностический unknown/gap.
- UI/CLI используют один контракт; protocol details остаются в backend.
- План, настройки, таблицы, статус и журнал согласованы.
- Никакие planned/ignored/mocked значения не названы live-доказательством.

## 6. Короткий стартовый запрос нового чата

> Обнови OPCLook по docs/llm-update-routing.md. Сначала выполни R2.0:
> сверяй locked библиотеки, назначь владельца каждой способности и используй
> готовый API/config. Выбери маршруты по задаче; напиши тесты по стабильным ID
> из docs/testing/acquisition-truth-tables.md через настоящие entry points.
> UA и DA должны использовать native SDK. Смена кандидата разрешена.
> Расширяй продуктовые механизмы; не пиши собственный Publish/recovery/COM слой.
> Отдельно сообщи, что не проверено на Windows/DA/live. Обнови документы и коммит.
