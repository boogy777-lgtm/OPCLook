# N1: Softing OPC Classic SDK 4.47.1 — проверка кандидата

Дата: 2026-10-30. Этот документ и тесты не подключают SDK в OPCLook и не
завершают миграцию DA. Исследован точный upstream commit
[`5130e763998189c822882434c43e6be2eb0174cf`](https://github.com/SoftingIndustrial/OPC-Classic-SDK/tree/5130e763998189c822882434c43e6be2eb0174cf)
(release v4.47.1); корневая [лицензия MIT](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/LICENSE).
Лицензии фактического транзитивного набора исходников ещё надо подтвердить;
корневая лицензия сама по себе не является полным SBOM.

## Покрытие API и границы доказательств

| Возможность | Первичный источник и вывод | Статус |
| --- | --- | --- |
| Сессия / remote / browse | [DaSession](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/C%2B%2B/src/client/Da/ClientDaSession.h) имеет URL, browse и отдельный class context; core `SODaCEntry::addServer` принимает URL или server/node/CLSID | API есть; remote DCOM/live не проверен |
| Sync / native Async | [SODaCGroup.h](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/include/SODaCGroup.h) объявляет syncRead/syncWrite и asyncRead/asyncWrite с transaction/cancel и per-item HRESULT | Декларации есть; реальные transport и completion не проверены |
| Cache / Device | maxAge `0` и `0xffffffff` определены в [ClientObjectSpaceElement.h](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/C%2B%2B/src/client/ClientObjectSpaceElement.h) | Проверен compile-only API, не конкретный сервер |
| Subscription и параметры | `DaSubscription` имеет отдельные `handleDataChangedEx`, `handleReadCompleted`, `handleWriteCompleted` и requested/revised rate | SDK предоставляет точки; ownership/teardown и late callback ещё требуют real linked test |
| Typed values/status/time | `ValueQT` содержит Variant, quality и DateTime; [OTClient.h](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/include/OTClient.h) передаёт per-item `long` и OTCValueData | Объявления сохраняют данные; полная конверсия VARIANT/SAFEARRAY и сохранность vendor bits не доказаны |
| Изоляция и x86 | [SOClient_VS2008.vcproj](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/demo/SOClient/SOClient_VS2008.vcproj) — Win32 MFC sample с прямой компиляцией core; C++ facade sample линкует TBC и OTB | Сам manifest не является успешной x86 сборкой или proof session isolation |

Найдены две отсутствующие в дереве ссылки sample manifest на `.mc` файлы;
их влияние на сборку не доказано. Официальные заголовки ошибок присутствуют.
В этой Linux-среде реальная попытка проверить MFC `StdAfx.h` завершается
`afxwin.h: No such file or directory`. Это ограничение среды, не доказательство,
что SDK нельзя собрать на Windows. Релиз не содержит готовых binary assets;
нужен воспроизводимый Win32 core build/link/init/cleanup без sample UI,
затем проверка server/teardown и типов. Не менять приватную активацию.

CI run 41062539737 дошёл до Win32 C++ и показал несовместимость probe с API
`SOCmnPointer::isNull` и конфликт имён встроенного Expat и текущего WinSDK.
Проба использует публичный `isNotNull`. Следующий run 41063686408 показал,
что DA Browse вызывает `SOCmnVariant::getVARTYPE`, требующий CMN_XML;
предыдущее отключение common XML было ошибочным и отменено. Watch и SOAP
остаются выключены только в тестовом profile. Точный embedded Expat теперь
компилируется как три отдельных C translation units вместо SDK C++ TU: SDK
`winconfig.h` задаёт XML_NS/XML_DTD и narrow ABI с `XML_STATIC`; на Linux
реальный C parser/link smoke проверил namespace, DTD и C-to-C++ callback ABI.
В CI 41065211900 Windows x86 компиляция SDK core и Expat прошла; core link
остановился на host hook `getInstanceHandle`, до запуска parser или callback.
SDK sample предоставляет handle своего приложения для FormatMessage; test
host использует Win32 `GetModuleHandle(nullptr)` и не содержит SDK message
resource table. Наличие локализованных текстов ошибок этим не доказано.
Первое локальное воспроизведение ошибочно использовало неполный заголовок;
он был восстановлен и проверен по Git blob перед успешным smoke.

## SDK defect: неверное сопоставление HRESULT после неизвестного handle

В [SODaCCallback.cpp](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/SODaC/SODaCCallback.cpp)
`handleReadAdvise` уплотняет известные items, values, qualities и timestamps,
но передаёт исходный массив errors; в `OnWriteComplete` callback получает
уплотнённые errors, однако `item->setWrite` индексирует исходный массив
уплотнённым индексом. Поэтому неизвестный handle перед известным может
перенести HRESULT другого элемента на известный. Это дефект SDK, не повод
добавлять workaround в App/Engine.

`reproduce-softing-status.py` проверяет Git blob `59205bb9b6e32641c966d334589fe355bafe273f`,
компилирует точное тело SDK read-метода и точный write-фрагмент в изолированном
fake окружении. Проверяются item identity, value, quality, timestamp, callback и
item cache outcomes и неизменность входного массива. Два read режима × семь
шаблонов handle дают 14 read случаев; семь write шаблонов при этом прогоняются
дважды. Оригинал: 6 read и 6 write несовпадений; предлагаемый
`softing-status-alignment.patch`: 0 и 0. Patch компактирует только отдельный
массив результатов и не меняет borrowed input; это заготовка исправления
upstream, **не** принятие библиотеки или модификация production SDK. Эти тесты
не линкуют полный SDK и не проверяют COM, shutdown или реальный DA server.

```sh
SOFTING_SDK_ROOT=/path/to/exact/sdk docs/testing/native-sdk/check-softing-api.sh
SOFTING_SDK_ROOT=/path/to/exact/sdk python3 docs/testing/native-sdk/test-core-features.py
SOFTING_SDK_ROOT=/path/to/exact/sdk docs/testing/native-sdk/test-expat-link.sh
SOFTING_SDK_ROOT=/path/to/exact/sdk python3 docs/testing/native-sdk/reproduce-softing-status.py --expect-existing-defect
SOFTING_SDK_ROOT=/path/to/exact/sdk python3 docs/testing/native-sdk/reproduce-softing-status.py --patched
```

Следующий обязательный gate: собрать настоящий SODaC/SOClt/SOCmn Win32,
вызвать реальный SDK callback на same-class наборах до/после patch, проверить
init/cleanup и raw status, затем физический DA/live, лицензии всего closure,
late callback/epoch и регрессии совместимости. Внешний issue/PR не отправлен.
