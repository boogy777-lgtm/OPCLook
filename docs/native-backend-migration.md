# R2.NATIVE — обязательная миграция на нативные OPC-клиентские библиотеки

Дата: 2026-10-30. Основание: прямое указание владельца проекта.
Статус: требование и план миграции; замена runtime этим коммитом не выполнена.
Вход: [routing table](llm-update-routing.md).
Этот документ уточняет прежний план: самостоятельное дописывание OPC data-plane
в приложении не является допустимым способом закрытия пробелов библиотеки.

## 1. Обязательный результат

OPC UA и OPC DA полностью используют готовые нативные клиентские библиотеки для
операций протокола и его жизненного цикла. Смена кандидата разрешена владельцем;
повторное подтверждение выбора кандидата в рамках этой задачи не требуется.
Проверить фактический API и совместимость до переноса production path.

Нативная библиотека здесь — Rust/native клиентский SDK, который реализует
соответствующие OPC операции. windows-rs и generated COM declarations —
ABI-инструменты, а не завершённая OPC DA клиентская библиотека.
Вынести старые прямые COM-вызовы в новый файл и назвать его SDK — не миграция.

## 2. Что обязана покрывать библиотека

| Слой | Ответственность |
| --- | --- |
| UA native SDK | SecureChannel/Session, service calls, Publish/acks, monitored items, protocol recovery и request lifecycle |
| DA native SDK | OPC server/group/items, native sync/async операции, callbacks/transaction, COM ownership/lifetime |
| Тонкий адаптер OPCLook | Конфиг -> готовый API; native typed result -> общий contract; capability и identity mapping |
| Engine OPCLook | Продуктовая политика режима, объединение целей чтения, fairness/backpressure, GUI/CLI command correlation |
| UI/CLI | Настройка соединения и представление фактов/ошибок; протокол не реализуют |

Если SDK сам предоставляет нужный scheduler/coalescing, сначала использовать
его. Engine не должен строить вторую таблицу того же wire lifecycle.
Отдельная продуктовая RequestId допустима только как логическая корреляция,
с однозначным mapping к native request/transaction.

## 3. Запрещённые способы закрытия пробелов

- Собственные IOPC client calls, COM callbacks, VARIANT memory owners и DA
  async transaction engine в production OPCLook вместо клиентской библиотеки.
- Второй UA Publish/reconnect/Republish/Transfer loop поверх живого SDK loop.
- Vendor-name исключения, скрытое переключение Subscription/Poll и выдуманные
  effective параметры.
- Emulation подписки циклическим Read под названием Subscription.
- Сведение неизвестного/Uncertain/Good subtype к безусловному success.
- Дублирование готовой library capability только потому, что API ещё не изучен.

Безопасная валидация внешних данных, типизированная диагностика Unsupported и
сохранение unknown/gap — обязательные части адаптера, а не протокольная эмуляция.

## 4. Допустимый выбор при пробеле

| Проверенный факт | Решение |
| --- | --- |
| Нужное поведение уже есть в публичном API | Использовать API |
| Есть штатный параметр SDK | Настроить и показать effective result |
| SDK выдаёт нужные данные другим типом | Тонкое преобразование без повторной реализации протокола |
| Обнаружен воспроизводимый дефект SDK | Исправление на уровне самой библиотеки либо другой кандидат; не обход в App/Engine |
| Capability отсутствует | Сменить библиотеку; до готовности честно Unsupported/disabled |
| Библиотека не даёт проверить обязательный invariant | Кандидат не принят; продолжить сравнение |
| Library fork нужен для устранения дефекта | Изменение локализовать в библиотеке с регрессией; явно показать владельцу scope, не прятать копию SDK в приложении |

Дефект библиотеки сначала воспроизводится минимальным тестом. Локальный fork не
является разрешением писать vendor-specific обходы и должен иметь ясный
upstream/replacement путь. Новая зависимость не считается доказательством качества.

## 5. Кандидаты и текущая позиция

| Протокол/кандидат | Позиция | Недостающие доказательства |
| --- | --- | --- |
| UA async-opcua 0.19.0 | Текущий native SDK, сохранить первым кандидатом | Exact locked source/trace recovery, revised state и raw write outcomes |
| DA opc_da 0.3.1 | Первый кандидат для feasibility по опубликованным callback/AsyncIo API | Реальный stream, версии методов, типы/status/time, cleanup, x86 Windows |
| DA opc_da_bindings / rust_opc safe client facade | Альтернативный native facade для сравнения | Покрытие нужных функций/версии, полноценность безопасного API, Windows/lifetime |
| DA opc-da-client / bytehound-opc-da-client | Дополнительные кандидаты | Async Rust facade не доказывает native Async/Subscription; нужны те же проверки |

Первоисточники кандидатов: [opc_da API](https://docs.rs/opc_da/latest/opc_da/client/index.html),
[rust_opc repository](https://github.com/ronbb/rust_opc),
[opc-da-client API](https://docs.rs/opc-da-client/latest/opc_da_client/),
[bytehound API](https://docs.rs/bytehound-opc-da-client/latest/opc_da_client/).
Фактический выбор фиксируется по контрактным проверкам, а не по названию crates.

## 6. Проверка перед выбором DA

1. Проверить API и закрепить версию; подтвердить реальный COM I/O, а не только await.
2. Проверить Browse/Read/Write/Subscription, Cache/Device, Sync/Async,
   initial result, per-item failures, quality/timestamps и raw HRESULT/status.
3. Проверить x86 Windows, локальную и удалённую сессию, callback после shutdown,
   re-add/late response, bounded cleanup и изоляцию двух сессий.
4. Прогнать контрактные таблицы C/U/E/S/T/A/O через native backend.
5. Выбрать библиотеку, которая покрывает обязательные способности без ручного
   клиентского COM слоя в OPCLook. Отсутствующие необязательные функции отключить явно.

## 7. Порядок миграции

| Шаг | Результат |
| --- | --- |
| N0 | Инвентаризация: операция -> native API/version -> нынешний собственный путь |
| N1 | Feasibility двух DA кандидатов; выбрать один по доказательствам и закрепить |
| N2 | Adapter за существующим contract; GUI/CLI и Engine не зависят от конкретного SDK |
| N3 | Контрактные проверки и Windows interop выбранного пути |
| N4 | Переключить production DA на native SDK; удалить старый OPC COM data-plane |
| N5 | UA: использовать имеющийся SDK API/config, удалить доказанные дубликаты |
| N6 | Настройки соединения, truth tables, status/devlog/audit и CI точного HEAD |

Не вводить два постоянных backend одновременно для одних и тех же тегов.
Изолированное сравнение на тестовом сервере разрешено.
Удаление старых файлов идёт только после анализа их использования; системные
WinAPI операции сети/UAC/nethelper не удаляются под видом OPC data-plane миграции.

## 8. Критерий завершения

Production paths UA и DA используют native SDK. В OPCLook не остаётся своей
реализации OPC transport/session/callback lifecycle. Все библиотечные пробелы
закрыты API/config, библиотечным исправлением или сменой кандидата.
Общие политики и контракт не разветвлены по именам библиотек.
Проверки O30–O35 и остальные затронутые строки пройдены на реальных entry points;
физические Windows/DA ограничения перечислены честно.
