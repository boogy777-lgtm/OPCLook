# Сетевая привязка и DA route steering

Документ описывает фактическую реализацию (2026-10-26). Исходные планы:
[`network-binding-plan.md`](network-binding-plan.md) и более новый
[`plan-ua-search-network.md`](plan-ua-search-network.md). При конфликте
приоритет у `plan-ua-search-network.md`.

## 1. Выбор адаптера

В `settings.json` хранится снимок адаптеров и явный выбор пользователя:

```json
{
  "preferred_adapter": "aabbccdd-1122-3344-5566-77889900aabb",
  "selected_local_ip": "192.168.1.20",
  "cached_adapters": [
    {
      "id": "aabbccdd-1122-3344-5566-77889900aabb",
      "name": "Ethernet",
      "description": "Intel(R) Ethernet Connection I219-LM",
      "state": "up",
      "addresses": [{ "ip": "192.168.1.20", "prefix": 24, "scope_id": null }],
      "gateways": ["192.168.1.1"],
      "if_index": 11,
      "ipv6_if_index": 17,
      "luid": 123,
      "metric": 25,
      "is_physical": true
    }
  ]
}
```

Правила:

- при старте список читается только из `cached_adapters`; Windows **не
  опрашивается**;
- пустой/удалённый/старый конфиг = пустой список до нажатия «Обновить»;
- «Обновить» полностью заменяет `cached_adapters` текущим инвентарём Windows,
  сохраняет JSON и пишет диагностику в журнал;
- в dropdown попадают только уникальные `AdapterId`, Up + есть пригодный
  не-loopback адрес; down/no-address/loopback/`UNSPECIFIED`/дубли исключаются
  с причиной в журнале;
- `AdapterId` — connection GUID из `AdapterName`; `NetworkGuid` не используется:
  это идентификатор network profile, который может совпадать у разных адаптеров;
- `AdapterSelection::None` — адаптер не выбран;
- `AdapterSelection::Adapter(GUID)` — выбран ровно один адаптер;
- автоматического выбора и fallback нет;
- stale GUID даёт «не выбран», ноль выбранных строк и кнопку «Забыть»
  (очищает `preferred_adapter` и `selected_local_ip`, но сохраняет кэш);
- при выборе адаптера сразу сохраняется конкретный локальный IP из этого
  адаптера; чужой сохранённый IP игнорируется;
- один адрес показывается read-only, несколько — ComboBox только с адресами
  выбранного адаптера;
- если адаптер пропал/выключен/без адреса — явная ошибка, без переключения на
  другую карту;
- loopback-поиск и loopback-подключение разрешены без выбранного адаптера;
- удалённый поиск/подключение, LDS/mDNS и подсеть требуют выбранного адаптера;
- смена адаптера не перепривязывает существующие сессии: они сохраняют
  захваченный `EndpointSpec.ua_bind`/lease;
- `GetAdaptersAddresses` вызывается только из обработчика «Обновить»; вкладка,
  `current_network_binding`, `discover_da_all`, `prepare_da_route_for_host` и
  старт subnet scan его не вызывают.

UI: вкладка «Сетевые настройки» содержит только ComboBox адаптера (имя + тип),
«Обновить», локальный IP, короткий статус/hint, «Забыть» для stale GUID и
«Закрыть». Описание, состояние, все адреса, шлюз, метрика, «Используется»,
«Маршрут DA» и сырая ошибка сети в форме не выводятся — они пишутся в журнал.

## 2. UA binding

`src/bff/net.rs`:

- `NetworkBinding` — неизменяемый снимок: локальный IP, IPv4 `IfIndex`, IPv6
  `Ipv6IfIndex`, scope id для link-local;
- `BoundTcpConnector` создаёт новый TCP-сокет на каждый `connect()`;
- перед `connect()` сокет получает `IP_UNICAST_IF`/`IPV6_UNICAST_IF` и
  `bind(local_ip:0)`;
- loopback-адреса всегда используют loopback source и не применяют interface
  option;
- сокет не переиспользуется между сессиями, discovery и сканером.

Привязка применяется к:

- `FindServers`, `FindServersOnNetwork`, `GetEndpoints`;
- созданию/активации UA-сессии и реконнектам;
- browse/read/write/subscription;
- сканированию подсети;
- mDNS (через `InterfaceIndex`).

`EndpointSpec.ua_bind` захватывается при открытии вкладки и не сериализуется.
Реконнект использует тот же снимок и не перепривязывается молча.

### LDS и mDNS

LDS не обязателен. Прямой `GetEndpoints` по адресу работает без LDS. Кнопка
«Поиск через LDS/mDNS» — отдельный источник и требует выбранного адаптера.
mDNS использует `DnsServiceBrowse`/`DnsServiceResolve` с `InterfaceIndex`;
результаты проверяются через `GetEndpoints` и отбрасываются, если не отвечают
как OPC UA.

### Сканер N+1

Порядок целей: `127.0.0.1` → `::1` (для IPv6) → локальный IP → адреса подсети.
FIFO, до 32 одновременных проверок, TCP precheck + UA handshake в одном
соединении. Подсеть больше 4096 адресов требует подтверждения и не сужается
молча. Остановка сохраняет частичные результаты. Один сервер с разными
адресами объединяется по `ApplicationUri`, endpoints объединяются (union), а не
теряются.

## 3. Лимит UA-сессий

`MAX_UA_SESSIONS = 100` в `src/constants.rs`. Проверка выполняется в
`App::open_tab_for_endpoint` до создания нового контейнера:

- считаются все UA `SessionView`, включая скрытые и отключённые;
- DA-сессии не считаются;
- реконнект/повторное открытие существующей вкладки не отклоняется;
- при 100 новая сессия не создаётся, показывается локализованная ошибка;
- удаление сессии освобождает слот;
- в UA-вкладке отображается счётчик `UA-сессии: N/100`.

## 4. DA route steering

DCOM не умеет привязывать COM-соединение к адаптеру. Для DA используется
временный host-маршрут `/32` (IPv4) или `/128` (IPv6) через выбранный
интерфейс.

### Поток

```text
resolve DA host
  → loopback/local IP? DA без маршрута
  → для каждого IP:
      лучший маршрут уже через выбранный адаптер? использовать, helper не нужен
      host-маршрут через другой адаптер? ошибка конфликта, не перезаписывать
      host-маршрута нет? helper добавляет /32 или /128
  → проверить GetBestRoute2
  → подключить DA-сессию
  → держать lease, пока сессия существует
  → освободить при disconnect/remove/закрытии приложения
```

Маршрут добавляется до `Discover`/`Connect`, а не BFF-воркером. После
`Servers`/`Error` для discovery и после `Connect`/`Disconnect`/удаления
сессии refcount уменьшается.

### Параметры маршрута

- IPv4 `/32`, IPv6 `/128`;
- `InterfaceLuid` выбранного адаптера;
- on-link — next hop `0.0.0.0`/`::`;
- off-link — gateway выбранного адаптера;
- `MIB_IPPROTO_NETMGMT`, метрика 1, non-persistent;
- конечный lifetime (`DA_ROUTE_LIFETIME_SECS`, 1 час) и периодическое
  обновление (`DA_ROUTE_REFRESH_SECS`, 30 минут) через helper;
- существующий host-маршрут через другую карту не удаляется и не
  перезаписывается.

Несколько DA-сессий к одному IP используют один маршрут; refcount ведётся по
`(IP, adapter GUID)`. Удаление маршрута выполняется только после последней
ссылки.

### Helper `opclook-nethelper`

Отдельный workspace-crate `nethelper/`:

- exe `opclook-nethelper.exe`, 32-битная сборка вместе с приложением;
- манифест `requireAdministrator`;
- зависимости: `windows`, `serde`, `serde_json`;
- запуск по требованию через UAC (`ShellExecuteExW("runas")`), не служба;
- named pipe: случайное имя, ACL только для текущего пользователя (SID),
  имя генерируется BCryptGenRandom. Helper проверяет PID клиента (= PID
  приложения) и SID; приложение проверяет PID сервера pipe по handle процесса,
  полученному при запуске. Проверка завершается до обмена командами, секрет в
  командной строке не передаётся. Защита не рассчитана на внедрение кода в уже
  аутентифицированный процесс. GUI и helper должны иметь одинаковую версию
  протокола; Windows runtime и UAC проверяются в CI/на стенде;
- принимает только `hello`/`add`/`remove`/`refresh`/`shutdown` для
  host-маршрутов; `add` создаёт маршрут, `remove` удаляет только созданный
  этим helper, `refresh` продлевает lifetime только своего маршрута;
  target — только unicast host, prefix строго `/32` или `/128`, next hop —
  unspecified/gateway того же семейства;
- отклоняет default/multicast/broadcast/loopback цели;
- пишет audit log в `%APPDATA%\opclook\nethelper-audit.log`;
- при закрытии pipe удаляет созданные маршруты; при падении helper маршруты
  истекают по lifetime.

### Отказ UAC

Если маршрут уже идёт через выбранный адаптер, DA разрешён без helper. Иначе
пользователь получает явную локализованную ошибку (`Не удалось подготовить
маршрут DA: …`); молчаливого использования другой карты нет. При запуске
helper приложение синхронно ждёт до `DA_HELPER_START_TIMEOUT` (2 с), включая
короткие sleep-интервалы: это ограниченная блокировка вызывающего потока, а не
бесконечное ожидание ответа UAC. Если за это время helper не подключился,
пользователь получает просьбу повторить подключение; сам UAC-запрос при этом
может оставаться на экране.

### Ограничения DA

- маршрут действует на весь компьютер для данного IP;
- DCOM может использовать дополнительные адреса; маршрутизируются все адреса,
  в которые разрешился хост;
- текущий DA-клиент — синхронный polling без `IOPCDataCallback`; асинхронные
  обратные вызовы потребуют отдельного решения;
- WFP-драйвер не используется.

## 5. Сборка и упаковка

```sh
cargo build --release --bin opclook --target i686-pc-windows-msvc
cargo build --release --bin opclook-nethelper --target i686-pc-windows-msvc
```

Упаковка обоих exe:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1
```

Результат: `target\i686-pc-windows-msvc\release\package\` с
`opclook.exe`, `opclook-nethelper.exe`, `README.md` и manifest helper.

`opclook-nethelper.exe` должен лежать рядом с `opclook.exe` (или путь можно
задать переменной `OPCLOOK_HELPER_EXE`).

## 6. Ручная проверка DA (требует стенда)

Живая DA-проверка в текущей среде невозможна: нет Kepware и второго DA-хоста.
Код при этом подготовлен; ниже — точный порядок проверки на стенде.

1. Собрать release (см. выше) и положить helper рядом с приложением.
2. Выбрать адаптер в «Сетевые настройки».
3. Подключиться к DA-хосту, доступному только через выбранный адаптер
   (второй хост/VM, не loopback).
4. Принять UAC. Проверить маршрут:
   ```powershell
   Get-NetRoute -DestinationPrefix <target-ip>/32
   ```
   `InterfaceAlias` должен совпадать с выбранным адаптером,
   `NextHop` — `0.0.0.0` для on-link или gateway.
5. Проверить фактический локальный адрес DCOM-соединения:
   ```powershell
   Get-NetTCPConnection -RemoteAddress <target-ip> |
     Select-Object LocalAddress, RemoteAddress, State
   ```
   `LocalAddress` должен принадлежать выбранному адаптеру.
6. Отключить/удалить DA-сессию и убедиться, что `/32` исчез.
7. Повторить с отклонённым UAC: без готового маршрута DA должен завершиться
   локализованной ошибкой; при уже существующем правильном маршруте — работать.
8. Сменить адаптер при активной DA-сессии: новая сессия к тому же IP должна
   получить ошибку конфликта, пока старая держит маршрут.

Для unit/elevated-тестов helper используйте только документированный стенд;
автоматический live DA-тест не считается выполненным.

## 7. Проверки

```sh
cargo test --lib --target i686-pc-windows-msvc
cargo test --test ui_interactions --target i686-pc-windows-msvc
cargo test --test no_hardcoded_ui_strings --target i686-pc-windows-msvc
cargo check --all-targets --target i686-pc-windows-msvc
cargo fmt --check
```

Unit-тесты route planning/refcount/conflict не требуют elevation. Elevated
add/verify/remove helper и live DA выполняются только вручную на стенде.

Фактический live-прогон 2026-10-26: `ua_search --ignored` — 2 passed (CODESYS,
оба endpoint); `ua_smoke --ignored` — 2 failed с таймаутом активации UA-сессии
(сервер отвечает на `GetEndpoints`, но не активирует сессию в текущем
состоянии). Live DA не выполнялся.
