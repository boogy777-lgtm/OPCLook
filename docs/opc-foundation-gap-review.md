# R2.FND — проверка пробелов по OPC Foundation

Дата проверки: 2026-10-30. База кода: `main@4738cbd9ace14bdd3f59808f40ffa7f8c635a2f0`.
Вход для следующего агента: [LLM routing table](llm-update-routing.md).
Связаны [R2](value-refresh-plan.md), [конфиг](connection-acquisition.md),
[таблицы тестов](testing/acquisition-truth-tables.md).

Это исследование опубликованной документации и исходников, не проверка
соответствия продукта. Никаких новых возможностей runtime этим коммитом не добавлено.

## 1. Достоверность и границы

- N: публичный текст OPC UA Part 4/5, нормативный источник.
- I: официальный обзор OPC Foundation по Classic, информационный источник.
- SDK: документация/код библиотеки; описывает механизм реализации, не заменяет N.
- P: выбранная политика OPCLook; не выдавать её за требование OPC.
- OPEN: нет достаточной проверки реализации или полного документа.

Публичная UA Reference при проверке показывала Part 4 v1.05.07.
Полные DA 2.05a/3.00 на странице Foundation помечены Paid Members; в этой
сессии они не получены. Публичный японский обзор DA 1.0 прочитан как I:
он не доказывает все детали DA 2.0/3.0. Найденный URL старого обзора DA 2.0
вернул 404; поисковый фрагмент не принят за проверенный текст полной спецификации.

## 2. Реестр первичных источников

| ID | Источник/раздел | Тип |
| --- | --- | --- |
| F01 | [Part 4, 6.7 Re-establishing connections](https://reference.opcfoundation.org/specs/OPC-10000-4/6.7) | N |
| F02 | [Part 4, 5.14.6 Republish](https://reference.opcfoundation.org/specs/OPC-10000-4/5.14.6) и [Part 5, 9.2 ResendData](https://reference.opcfoundation.org/specs/OPC-10000-5/9.2) | N |
| F03 | [Part 4, 7.10 DataChangeTrigger](https://reference.opcfoundation.org/specs/OPC-10000-4/7.10) и [7.22.2 DataChangeFilter](https://reference.opcfoundation.org/specs/OPC-10000-4/7.22.2) | N |
| F04 | [Part 4, 5.13.1.5 Queue parameters](https://reference.opcfoundation.org/specs/OPC-10000-4/5.13.1.5) и [7.21 MonitoringParameters](https://reference.opcfoundation.org/specs/OPC-10000-4/7.21) | N |
| F05 | [Part 4, 5.14.2 CreateSubscription](https://reference.opcfoundation.org/specs/OPC-10000-4/5.14.2) | N |
| F06 | [Part 4, 5.11.4 Write](https://reference.opcfoundation.org/specs/OPC-10000-4/5.11.4) и [7.38 StatusCode](https://reference.opcfoundation.org/specs/OPC-10000-4/7.38) | N |
| F07 | [Part 4, 5.11 Read](https://reference.opcfoundation.org/specs/OPC-10000-4/5.11) и [5.7.5 Cancel](https://reference.opcfoundation.org/specs/OPC-10000-4/5.7.5) | N |
| F08 | [Part 5, 6.3.11 OperationLimitsType](https://reference.opcfoundation.org/specs/OPC-10000-5/6.3.11) | N |
| F09 | [Part 4, 5.9.5 RegisterNodes](https://reference.opcfoundation.org/specs/OPC-10000-4/5.9.5) | N |
| F10 | [Part 4, 5.13.2 CreateMonitoredItems](https://reference.opcfoundation.org/specs/OPC-10000-4/5.13.2) и [7.25.2 Initial notification](https://reference.opcfoundation.org/specs/OPC-10000-4/7.25.2) | N |
| F11 | [Part 4, 7.11 DataValue](https://reference.opcfoundation.org/specs/OPC-10000-4/7.11) | N |
| F12 | [Classic Data Access, доступные версии](https://opcfoundation.org/developer-tools/specifications-classic/data-access/) | Официальный каталог |
| F13 | [Официальный японский обзор DA 1.0, 1997, §§3.4.3–3.4.3.5, печатные стр.25–27](https://opcfoundation.org/wp-content/uploads/2025/10/jp_library_gaiyov1.pdf) | I, PDF pages 29–31 |
| F14 | [async-opcua Session](https://docs.rs/async-opcua-client/latest/opcua_client/struct.Session.html), [SessionEventLoop](https://docs.rs/async-opcua-client/latest/opcua_client/struct.SessionEventLoop.html), [connect source](https://docs.rs/async-opcua-client/latest/src/opcua_client/session/connect.rs.html) | SDK; moving URLs, версия source требует сверки |

F14 содержит moving URLs. Поисковый/API результат показывал 0.19.0, но открытый
connect source — 0.16.0. Поэтому точная recovery-реализация locked 0.19 не доказана.
Перед реализацией сверить Cargo.lock и исходники именно закреплённой версии;
не обновлять зависимость для удобства цитирования.
Копии нормативных документов в репозиторий не включались.

## 3. Что нужно дополнить или исправить

| Finding | Что установлено | Что меняется в решении OPCLook | Тесты |
| --- | --- | --- | --- |
| OF01 | F05 различает requested и revised параметры, lifetime/keepalive | Захватить фактические параметры из SDK; не называть requested значением effective | O01–O02 |
| OF02 | F03: STATUS_VALUE — default; timestamp-trigger относится к SourceTimestamp; deadband меняет его поведение | В UA extension добавить явный trigger, показывать effective filter. Не обещать уведомления при изменении только ServerTimestamp | O03–O05 |
| OF03 | F04: queue=1 сохраняет последний sample; очередь >1 имеет overflow signalling | Диагностическая таблица — latest-value view. Не обещать историю всех переходов; сохранять Overflow/status bits | O06–O07 |
| OF04 | F05: lifetime как минимум 3 x keepalive; пустой keepalive — не значение тега | Проверять revised счётчики; разделить связь канала, свежесть снимка и изменение процесса | O08–O09 |
| OF05 | F01/F02 описывают восстановление Session/Subscription и потерянных сообщений | Сначала определить владельца recovery в SDK. Republish/Transfer не реализовывать второй раз в Engine; потерю истории не объявлять устранённой снимком | O10–O13 |
| OF06 | F06 допускает Good_CompletesAsynchronously и частичные результаты; порядок повторных записей одного узла в пачке не определён | Сохранить сырой per-item статус и степень подтверждения Write. Timeout не доказывает отсутствие записи; автоматический повтор Write запрещён политикой P | O14–O17 |
| OF07 | F08 задаёт optional OperationLimits; F07 возвращает per-item результаты | Учитывать ограничения при chunking, сохранять исходы ключей. Не создавать второй splitter поверх существующего адаптивного | O18–O19 |
| OF08 | F09: RegisterNodes alias действует в Session, регистрация не проверяет существование NodeId | Alias — оптимизация адаптера, не identity ItemKey и не доказательство readable. Не переиспользовать alias новой Session | O20–O21 |
| OF09 | F10: initial notification и результат CreateMonitoredItems — разные события | Успех регистрации не доказывает Good value. Ранний callback учитывается по своей регистрации | O22 |
| OF10 | F13 позволяет сочетать sync/async чтение, refresh и subscription | Убрать наш ранее предложенный blanket-запрет DA Subscription + Sync Read. Разовые чтения допускаются по capability; два постоянных цикла по-прежнему исключены политикой P | C06, O23 |
| OF11 | F13 различает Cache/Device и active-state для DA 1.0; shared callback различается по transaction | Не считать все DA callback stream. Версионные Cache/Device/Refresh правила проверить по конкретному интерфейсу | O24–O26 |
| OF12 | F01 отделяет transport recovery от создания новой Session; F14 умеет session event loop/reconnect | Не отождествлять SecureChannel, remote Session и локальную Connect epoch; callback устаревшей stream registration отбрасывается адаптером | O12–O13, O27 |

## 4. Сопоставление с проверенным кодом

| Место | Наблюдение в базе | Вывод и граница |
| --- | --- | --- |
| crates/backend/Cargo.toml и Cargo.lock | async-opcua/client закреплены на 0.19.0 | SDK — первый кандидат на владельца Publish/recovery; точный source audit этой версии открыт |
| ua/session/subscription.rs::create_subscription | Удобный SDK-вызов возвращает sub_id; applied_interval_ms получает запрошенный interval_ms | Пробел в отображении negotiated данных; нельзя доказывать, что сервер принял requested |
| ua/session/mod.rs::monitoring_parameters | queue_size=1, discard_oldest=true | Latest sample, не архив; при queue=1 discard policy не создаёт историю |
| ua/values.rs::data_change_filter | StatusValue либо null filter | Timestamp-only уведомления не запрошены; отсутствие их не bug серверного stream |
| ua/io.rs::write | Все небадовые статусы сведены к Ok(()) | Теряются Good subtype/Uncertain distinction; это статическое наблюдение, не live воспроизведение |
| ua/session/mod.rs::timed_out | Таймаут помечает сессию для закрытия | Сначала проверить SDK timeout/cancel; baseline quarantine не убирать без доказанного безопасного lifecycle и регрессии |
| ua/session/subscription.rs::create_round | Уже есть bounded/adaptive monitor chunk | Расширять существующий механизм, а не новый универсальный batch framework |
| ua/session/connection.rs | Запускает SDK event loop | Отсутствие Republish-кода в App не доказывает отсутствия recovery внутри SDK |
| da.rs | SyncIO Cache, active group/items | Это реальный baseline; не выдать его за Device/Async/stream |
| opc/bindings/da.rs | Различные DA интерфейсы имеют различные сигнатуры | Generated binding не подтверждает рабочую реализацию и семантику сервера |

Эти выводы относятся к указанной базе. Рефакторинг может перенести символы:
следующий агент сначала находит актуальный символ через rg, затем меняет маршрут.

## 5. Важные поправки к прежним таблицам

1. C06 теперь разрешает DA Subscription + Sync Read при stream и sync-read
   capability. Ограничение UI Kepware не переносится в общий механизм OPCLook.
2. E09/S02 означают повторное планирование только если транспорт остаётся
   работоспособным. При UA quarantine после IO timeout сначала закрыть старую
   эпоху, завершить её работу и восстановить регистрацию; не слать в неё R2.
3. Snapshot после reconnect — восстановление текущего состояния, не восстановление
   истории. При потере последовательности gap остаётся диагностически видимым.
4. «Subscription без периодических Read» относится к Value тегов пользователя.
   SDK может читать служебные узлы для Session liveness. Такой keepalive не
   является скрытым polling технологических тегов и не обновляет их received_at.
5. Product default interval_ms>0 не является утверждением, что UA запрещает
   requestedPublishingInterval<=0: протокол допускает server revision.
6. FreshSource означает попытку получить источник; cache/best effort и raw status
   не скрываются. Это не автоматическая гарантия выполнения физического действия.

## 6. Что остаётся OPEN

- Полная сверка DA 2.05a/3.00 Refresh/SetEnable/Cancel/transaction rules по
  легально доступному полному тексту и реальному Windows стенду.
- Какие именно recovery, replay, gap и negotiated callbacks экспортирует
  закреплённый async-opcua 0.19 и что делает OPCLook при terminal event loop.
- Требуется ли расширение SDK API для фактических revised параметров. Наличие
  удобного метода только с sub_id не оправдывает подстановку requested.
- Проверка raw Write result contract, COM status и live поведения серверов.
- Физическая свежесть и полная история переходов не доказаны текущим Read/Poll
  или latest-value Subscription. Не добавлять эти обещания в UI.

## 7. Граница этой работы

Обновлены документы, routing и ожидаемые результаты. CSV G/D не меняют формулы:
протокольные детали уточняют входные capability/current_epoch и переходы O/E/S.
Проверки runtime и Windows COM не выполнялись; Cargo/rustfmt недоступны.
