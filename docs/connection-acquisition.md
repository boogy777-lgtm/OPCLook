# R2.3 — настройки получения данных соединения: UA и DA

Дата: 2026-10-30. База: `main@62ad187c2294cff5df55fe40e82b05f5dda25617`.
Статус: спецификация для реализации и тестов. Поля ниже ещё не добавлены в Rust/UI.
Основание: [план R2](value-refresh-plan.md).
Проверки: [таблицы решений](testing/acquisition-truth-tables.md).

## 1. Термины и граница механизма

В запросе «Push vs sub» принят смысл Poll vs Subscription из обсуждения R2.
Push и Subscription обозначают один способ постоянной доставки изменений.
Канонические значения UpdateMode: Subscription и Poll; третьего Push-режима нет.
Для DA пользовательская подпись: «Подписка на изменения (Exception)».

Способ постоянного получения данных и способ выполнения отдельного запроса —
разные оси. Асинхронное чтение завершается через callback, но от этого не
становится подпиской. Наличие callback не разрешает периодические Read в
Subscription и не доказывает получение изменений в Poll.

**Class:** постоянное получение и разовое наблюдение значений зарегистрированных
элементов соединения, независимо от OPC UA/DA.
**Inv:** единственная политика постоянного канала; каждое чтение имеет цель,
корреляцию и исход; конфиг не обещает неподдерживаемую backend-возможность.
**N+1:** другой протокол реализует адаптер возможностей и операций; Engine,
очередь потребностей, UI общих настроек и правила корреляции не переписываются.
**Guard:** таблицы U/C/D/G/E/S/T/M проверяются для обоих протоколов; ветвления
по Protocol внутри общего планировщика не заменяют capability-контракт.

## 2. Что подтверждено PTC, а что выбрано для OPCLook

| Первичный источник | Факт |
| --- | --- |
| [DA OPC Group](https://support.ptc.com/help/kepware/drivers/en/kepware/drivers/OPCDACLIENT/OPC_Group.html) | Exception использует активную группу и уведомления; Poll читает по интервалу. DA 2.0/1.0 async и sync Read Method отделены от Update Mode. |
| [DA Communications Parameters](https://support.ptc.com/help/kepware/drivers/en/kepware/drivers/OPCDACLIENT/communications-parameters.html) | Read after Write настраивается отдельно; по умолчанию включён. |
| [OPC Quick Client Operations](https://support.ptc.com/help/kepware/features/en/kepware/features/OPCQUICKCLIENT/operations.html) | Различаются Cache/Device и Sync/Async чтения. Async DA 2.0 использует connection points, DA 1.0 — IAdviseSink. |
| [UA Subscription](https://support.ptc.com/help/kepware/drivers/en/kepware/drivers/OPCUACLIENT/subscription.html) | Exception и Poll являются вариантами Update Mode. |
| [UA Reconnect](https://support.ptc.com/help/kepware/drivers/en/kepware/drivers/OPCUACLIENT/Reestablishing_Connections.html) | Восстановление снимка через разовое Read применяется и при подписке. |
| [UA Read](https://reference.opcfoundation.org/specs/OPC-10000-4/5.11) | maxAge направляет выбор кэша/источника; допустим best effort. |

Это документация Kepware как OPC-клиента. Выбор общего конфига, его границ и
поведения UI ниже — решение OPCLook, не описание уже существующего кода.
PTC не является доказательством соответствия произвольного DA-сервера стандарту.

## 3. Общий конфиг в EndpointSpec

Добавить `EndpointSpec.acquisition: AcquisitionConfig` в общий contract.
Имя поля и enum — контракт для нового агента; layout Rust уточнить без изменения
семантики. Сохранять конфиг вместе с конкретным recent endpoint/соединением.
Глобальные настройки больше не являются текущим источником режима/интервала.

| Поле | Тип/границы | Начальное значение нового профиля | Владелец смысла |
| --- | --- | --- | --- |
| update_mode | Subscription / Poll | UA: Subscription; DA: Poll до реализации DA callbacks | Общий механизм |
| interval_ms | Целое 1..3600000 | 1000 | Общий механизм; верхняя граница — решение OPCLook |
| read_after_write | bool | true | Общая политика; false отключает только автоматический read-back |
| poll_freshness | CacheAllowed / FreshSource | FreshSource | Общая цель чтения; доступность проверяет адаптер |

`interval_ms` в Poll задаёт период Read. В Subscription задаёт желаемую частоту
канала, не включает polling. Пересмотренный интервал хранится отдельно от
запрошенного и показывается пользователю; его нельзя сохранять вместо пожелания.

InitialSnapshot и восстановление — встроенные цели общего механизма, не новые
независимые переключатели. Их разовая попытка не запускает периодический цикл.
В Subscription без результата ждать один пересмотренный интервал; при отсутствии
допустимого пересмотренного интервала показывать ошибку конфигурации канала,
не заводить нулевой busy-loop.

Две независимые сессии одного endpoint могут иметь разные AcquisitionConfig.
Они не должны делить изменяемый конфиг по одному URL/ProgID. История recent
endpoints может хранить последний выбранный профиль, но изменение истории
не меняет snapshot уже работающих сессий.

### Расширения адаптеров

Не помещать DA COM-методы и UA MonitoredItems в общий планировщик.

| Адаптер | Поля расширения | Проверка |
| --- | --- | --- |
| UA | sampling interval, absolute deadband | finite/range и поддержка сервера; поля не используются DA |
| DA | read_method: Synchronous / Asynchronous; write_method с теми же значениями; percent_deadband: 0..100 finite | read/write capability отдельно; percent deadband применяется только к Subscription |

Для нового DA-профиля текущей реализации read/write method = Synchronous.
Это честное значение доступного backend, а не рекомендация PTC о производительности.
Когда Async появится, его можно выбрать явно. Внутри Asynchronous адаптер может
выбрать поддерживаемую версию DA 2.0/1.0 и показать effective method.
Нельзя молча заменить явно выбранный Async на Sync либо Subscription на Poll.
Точная версия COM интерфейса не является ещё одним режимом общего Engine.
Следуя ограничению UI Kepware, DA Subscription + Sync read_method не предлагать
как поддерживаемую комбинацию OPCLook; разовые Read этого профиля должны иметь
реальную async-реализацию. Это ограничение продукта, не общий запрет OPC DA.

Абсолютный deadband UA и процентный DA не копировать друг в друга.
При смене протокола валидировать его расширение заново.

## 4. UI соединения

Общий блок «Получение данных» в диалоге подключения и свойствах соединения:

- «Режим обновления»: «Подписка на изменения» / «Периодический опрос».
- «Интервал, мс» с подписью смысла выбранного режима.
- «Прочитать значение после успешной записи».
- В Poll: «Источник чтения»: «Допускается кэш» / «Запросить источник».
- Ниже: только поля активного адаптера; недоступность имеет причину capability.
- Показывать requested mode, effective channel, revised interval и последний
  origin отдельно. Read result не переводит failed stream в Ready.

Все строки — locale RU/EN/ZH. GUI строит конфиг, Engine валидирует и исполняет.
CLI принимает те же поля (`--update-mode subscription|poll`, `--interval-ms`,
`--read-after-write true|false`, `--poll-freshness cache|source`).
Старый `--poll` становится совместимым alias явного Poll-конфига, а не фильтром
push после создания подписки. `read NODE` остаётся разовой операцией, не циклом.

Активный конфиг неизменяем в течение эпохи Connect. Редактирование — черновик.
Cancel не меняет сессию. Apply сохраняет профиль для следующего подключения.
«Применить и переподключить» явно завершает старую эпоху и создаёт новую после
валидации. Не вводить параллельный live reconfigure и новый счётчик поколений,
если достаточно существующего reconnect/epoch-механизма.

## 5. Capability-контракт адаптера

Адаптер сообщает возможность stream, способов read/write и допустимой freshness.
Это факты реализации плюс результаты negotiation, не обещания из имени протокола.
Общий Engine работает с `validate(config, capabilities)`, `start_stream`,
`stop_stream`, `read(options)` и единым потоком результатов.
Новые запросы в обход Engine из GUI/CLI/backend refresh-путей запрещены.

| Политика | UA backend | DA backend |
| --- | --- | --- |
| Subscription | MonitoredItems/Publish | Активные group/items + подписка callbacks на data changes |
| Poll | Read, без monitored values | Read по выбранному методу; постоянные data changes не доставляются |
| Async Read completion | Origin::Reply(request_id) | OnReadComplete/эквивалент с transaction correlation -> Origin::Reply(request_id) |
| Stream notification | Origin текущего stream/Connect | OnDataChange текущей регистрации -> origin stream, не Reply Read |

Poll DA может требовать callback для завершения Async Read. Запрещена постоянная
доставка data changes, а не сам COM callback-интерфейс.
PTC использует неактивную группу/элементы с краткой активацией при polling;
не копировать эту деталь без проверки Cache/Device и конкретного интерфейса.
В OPCLook CacheAllowed допускается только при пригодном кэше адаптера; при
неактивном/невалидном кэше показывать ошибку/Bad, не изображать свежее значение.
FreshSource направляется в UA maxAge=0 или DA Device-операцию поддержанного
интерфейса. CacheAllowed не является утверждением о физической свежести.
Для UA CacheAllowed передаёт maxAge=effective interval; для DA запрашивает Cache
только при пригодном cache-state. InitialSnapshot использует тот же cache allowance,
а read-back/явный Refresh требуют FreshSource. При объединении FreshSource сильнее
CacheAllowed. Если FreshSource недоступен, read_after_write=true отклоняется;
недоступный ручной Refresh имеет причину, а не заменяется чтением Cache.

Один логический Read-слот на сессию; DA адаптер вправе ограничить суммарно
read/write, если этого требует интерфейс. Глобальный mutex между сессиями запрещён.

### Фактический DA baseline

В `crates/backend/src/bff/da.rs` проверенной базы:

- `create_group` вызывает AddGroup(active=true), требует IOPCSyncIO.
- `add_items` задаёт bActive=true.
- `read` вызывает IOPCSyncIO::Read(OPC_DS_CACHE).
- В этом worker нет реализованной доставки IOPCDataCallback/DA 1.0 stream.
  Наличие generated bindings IOPCAsyncIO/IOPCDataCallback не считается реализацией.

Следовательно текущий DA — чтение кэша по расписанию, а не готовый Exception.
Нельзя объявить его Subscription изменением названия или флагом настройки.
Для FreshSource потребуется Device-путь; для Async/Subscription — реальная COM
реализация, владение callback lifetime, transaction/epoch и проверки Windows.
До этого неподдержанные настройки видны, но недоступны с причиной.

## 6. Миграция и сохранение

| Вход | Требуемый результат |
| --- | --- |
| Старый UA endpoint без acquisition | Явный Subscription; interval из старого update_interval_secs, перевод checked; сохранить профиль |
| Старый DA endpoint без acquisition | Явный Poll + Sync + CacheAllowed, read_after_write=false; сохранить опубликованный baseline без автоматического read-back и показать migrated profile |
| Новый DA профиль | Poll + Sync + FreshSource и read_after_write=true после реализации Device capability; до неё FreshSource недоступен, CacheAllowed виден явно, read_after_write требует false либо отклоняется |
| Endpoint с корректным acquisition | Round-trip без замены глобальными defaults |
| Неизвестный update_mode или неверные единицы/границы | Ошибка настройки; не выбирать другой режим |
| Отсутствующий DA/UA extension | Совместимый default адаптера; никаких обещаний недоступных возможностей |
| Переполнение seconds -> milliseconds | Видимая ошибка миграции; не wrap и не нулевой интервал |
| Старый UA fallback/poll_only | Видимое предупреждение об изменении семантики; не скрытый переход Poll |

Неполный новый профиль не сохранять как будто он применён. Requested profile
можно хранить отдельно от действующего validated snapshot, с явной ошибкой.
read_after_write=false не меняет результат Write и не создаёт ложное подтверждение:
строка ждёт штатного stream/poll/ручного Refresh.

## 7. Порядок реализации и состояние

1. Contract/config/serde/capability validation и тесты C/U/M.
2. Engine queue/read slot/demand correlation и тесты G/D/E.
3. UA adapter и DA Device/Sync mapping; окна соединения и CLI общих настроек.
4. DA Async/Subscription только с callback lifetime и Windows COM стендом.
5. Тесты перестановок S/T, UI/locale/headless и live UA/DA; status/devlog/audit.

В этом коммите изменяется спецификация и данные таблиц. Реализация настроек в
соединениях — открытая работа R2.3; не считать добавленную документацию готовым UI.
