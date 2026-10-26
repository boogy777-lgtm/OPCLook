# Описание оригинального EleSy OPC Test Client 2.1 (реверс-инжиниринг)

Документ описывает восстановление и понимание оригинального бинарника
`D:\AlphaTools\OPCtools.exe` (EleSy OPC Test Client 2.1.0.139, Delphi, 2012,
32-bit, OPC DA 2.0) и его отображение на перезапись на Rust
(`opcda-egui`). Это справочный материал к README.md.

Исходные данные:

- Бинарь: `D:\AlphaTools\OPCtools.exe`.
- Артефакты: `C:\Users\b003311\Documents\opcda_re` — PE dump
  (`pe_dump.txt`), 16 DFM-форм (`dfm/*.dfm.txt`, бинари в `forms/*.bin`),
  декодеры `dfm_decode.py`/`handlers.py`/`events.py`/`pe_dump.py`,
  карты форм и обработчиков (`main_tree.txt`, `main_inventory.txt`,
  `dialogs_inventory.txt`, `handlers.json`/`handlers.txt`, `methods.json`/
  `methods.txt`), дампы дизассемблера `dis_*.txt`.

## Версия и метаданные

PE-resource версия (из `pe_dump.txt`):

- `FileVersion` / `ProductVersion` = `2.1.0.139`
- `ProductName` = `OPC Test Client`
- `FileDescription` = «OPC Test Client is intended to operate with multiple
  OPC servers via DA specification of version 2.0»
- `CompanyName` = `EleSy company`
- `LegalCopyright` = `Copyright (c) EleSy company. All rights reserved.`

Продукт 32-битный, собран Delphi; код использует 32-битную OPC DA COM.
Описания и UI — русские (`ЭлеСи Тестовый ОРС клиент`).

## Формы

Всего **16 форм**. В папке `forms` есть также `DVCLAL_16.bin` и
`PACKAGEINFO_2024.bin` — это не формы, а служебные записи Delphi
(DVCLAL = Delphi Version Control / "Delphi 2009+" marker, PACKAGEINFO —
информация о пакете); они не являются окнами.

| Форма (имя класса из DFM) | Файл DFM | Назначение |
|---|---|---|
| `TAboutBox` | `TABOUTBOX_2411` | О программе |
| `TFormAddSignal` | `TFORMADDSIGNAL_3215` | Добавить сигнал |
| `TFormColumn` | `TFORMCOLUMN_4431` | Настройка столбцов |
| `TFormCompiling` | `TFORMCOMPILING_14973` | Компиляция скрипта |
| `TFormEditPropScript` | `TFORMEDITPROPSCRIPT_11411` | Свойства скрипта |
| `TFormEditValueSignal` | `TFORMEDITVALUESIGNAL_2424` | Изменить значение сигнала |
| `TFormGoToLineNumber` | `TFORMGOTOLINENUMBER_1378` | Переход к строке |
| `TFormLoad` | `TFORMLOAD_1591` | Загрузка |
| `TFormMessage` | `TFORMMESSAGE_5488` | Сообщение |
| `TFormOPCtools` | `TFORMOPCTOOLS_178683` | Главная форма |
| `TFormOptions` | `TFORMOPTIONS_10198` | Параметры |
| `TFormSelectComputer` | `TFORMSELECTCOMPUTER_17289` | Выбор компьютера |
| `TFormSelectOPCServer` | `TFORMSELECTOPCSERVER_17630` | Выбор OPC сервера |
| `TFormSelectServer` | `TFORMSELECTSERVER_2950` | Выбор сервера |
| `TFormSetQuality` | `TFORMSETQUALITY_3540` | Установить качество |
| `TFormTerminate` | `TFORMTERMINATE_1043` | Завершение |

Список совпадает с целью в `delphi_map.py` (те же 16 имён). Пробелы между
формами, которые могли бы отличаться на разных исполнениях, не наблюдались.

## Восстановленные исходные модули

По строкам путей компиляции в `ascii_strings.txt` восстановлены имена
исходных `*.pas`-файлов (пути вида `Z:\OPCTools\src\...`):

| Модуль | Путь |
|---|---|
| `UnitValues.pas` | `Z:\OPCTools\src\Values\UnitValues.pas` |
| `UnitSignalOPCtools.pas` | упоминается как `UnitSignalOPCtools` |
| `UnitOPCServerInfo.pas` | упоминается как `UnitOPCServerInfo` |
| `UnitSelectOPCServer.pas` | `Z:\OPCTools\src\Forms\UnitSelectOPCServer.pas` |
| `UnitSelectServer.pas` | `Z:\OPCTools\src\Dialogs\UnitSelectServer.pas` |
| `Script.pas` | `Z:\OPCTools\src\Script\Script.pas` |
| `Main.pas` | `Z:\OPCTools\src\Main.pas` |

Дополнительно встречаются `UnitServerOPCtools.pas`,
`UnitSetQuality`, `UnitOptionsEditScript`, `UnitColorScript.pas`,
`UnitSplashSyntax.pas` (см. `ascii_strings.txt`), сторонний редактор
`SynEdit` (`Components\SynEdit-20011025-CVS\Source\SynTextDrawer.pas`) и
справочная система `help\OPCTools_ru.chm`.

## OPC DA интерфейсы

32-битный COM. Обработчики и DFM главной формы показывают операции
классического OPC DA 2.0:

- `IOPCServer` — создание/удаление группы, `GetStatus`, `GetErrorString`.
- `IOPCBrowseServerAddressSpace` — навигация и обзор адресного пространства
  (ветки/листья, `ChangeBrowsePosition`, `BrowseOPCItemIDs`, `GetItemID`).
- `IOPCItemMgt` — `AddItems`/`RemoveItems`, типы, права доступа.
- `IOPCSyncIO` — синхронные `Read` (кэш) и `Write` значений.
- `IOPCServerList` (OPCEnum) + категории `CATID_OPCDAServer20/10` —
  перечисление серверов.
- Точки подключения (`IConnectionPoint`) использовались в оригинале для
  асинхронного обновления значений/журнала; интерфейсы оповестителя — часть
  OPC DA 2.0, но в перезаписи не применяются (чтение только синхронное).

`dfm_decode` восстановил имена компонентов и событий главной формы:
`sgSignalTablo` (TStringAlignGrid, ColCount=9 — таблица сигналов),
`tvOPCSignals` (TTreeView — дерево), `reScript` (TSynEdit — редактор
скрипта), `reLog` (TMemo — журнал), панели `pnNaviScript`/`pnScriptLog`/
`pnSignalTable`, `StatusBar`. Инструменты: `tbtnDelSignal`, `tbSetQuality`
(выпадающее меню `pmSetQuality`), кнопки скрипта.

## Столбцы главного окна и типы

`sgSignalTablo` имеет 9 столбцов; из русских строк и меню восстановлено
примерное соответствие (порядок сверен с UI перезаписи):

| № | Назначение (по строке/подсказке) | Столбец в opcda-egui |
|---|---|---|
| 1 | индикатор состояния строки | Статус (`Status`) |
| 2 | полное имя сигнала | Полное имя (`FullName`) |
| 3 | записываемый пользователем тип | Записываемый тип (`WriteType`) |
| 4 | тип в сервере | Тип (`Type`) |
| 5 | текущее значение | Значение (`Value`) |
| 6 | характеристика достоверности | Качество (`Quality`) |
| 7 | время последнего изменения | Время изменения (`Time`) |
| 8 | программный идентификатор сервера | OРС сервер (`Server`) |
| 9 | описание сигнала | Описание (`Description`) |

В перезаписи строкой заголовка столбца 8 является `OРС сервер` (с кириллической
«Р»), что сохранено как у оригинала. Диалог «Настройка столбцов»
(`TFormColumn`) позволяет скрывать/отображать столбцы и менять порядок —
это перенесено в `ColumnSet` (`columns.rs`).

## Значения и качество

Диалог `TFormSetQuality` («Установить качество», `TFormSetQuality`) содержит
выпадающий список значений качества `DropDownCount = 17`:

плохое: 0 BAD, 4 CONFIG_ERROR, 8 NOT_CONNECTED, 12 DEVICE_FAILURE,
16 SENSOR_FAILURE, 20 LAST_KNOWN, 24 COMM_FAILURE, 28 OUT_OF_SERVICE,
64 UNCERTAIN, 68 LAST_USABLE, 80 SENSOR_NOT_ACCURATE, 84 EGU_EXCEEDED,
88 SUB_NORMAL, 96 CONVERT_ERROR; хорошее: 192 GOOD, 200 CALCULATED,
216 LOCAL_OVERRIDE.

Это ровно совпадает с таблицей `da_quality_text` в `src/bff/model.rs`
(кодируется старшими битами: 0x40=UNCERTAIN, 0xC0=GOOD; младшие биты
отбрасываются маской `code & 0xFFFC`). Формы значений — COM/VARIANT:
BOOLEAN, I1..I8/U1..U8, R4/R8, BSTR, DATE и массивы; конвертация воспроизведена
в `src/bff/da_variant.rs`.

## Параметры по умолчанию

`TFormOptions` («Параметры», вкладки «Основные»/«Расширенные»):

- Интервал обновления значений: 1..60 с, рекомендовано 1 с, по умолчанию 1.
- Автовосстановление связи с сервером (флаг).
- «Отображать сигналы различными цветами» (флаг).
- «Запрашивать подтверждение изменения сигнала» (флаг).
- Таймаут операций: допустимо 1..1800 с, рекомендовано 60.
- Интервал тестирования на разрыв связи: 1..1800 с, рекомендовано 60.

Эти значения перенесены в `Options` (`settings.rs`): `update_interval_secs=1`,
`auto_reconnect=true`, `show_colors=true`, `confirm_write=true`,
`operation_timeout_secs=60`, `test_interval_secs=60`. В оригинале также были
отдельные настройки цветов/подсветки скрипта (`TFormEditPropScript`,
`UnitColorScript.pas`), в перезаписи не воспроизведены.

## Язык скриптов

Редактор скрипта хранит файлы `.stx` (фильтр «script text|*.stx»), а также
`.sol` («SOL files (*.sol)»). Пример скрипта из `TFormEditPropScript`
(подсветка синтаксиса):

```
function UserFunc(Sig1, Sig2: Signal; Left, Right: Variant);
begin
  for i := Left to Right do
  begin
    SetValue(Sig1, i);
    SetValue(Sig2, i*13.67);
    Sleep(500);
  end;
end;

Program
Var SerVostok: Server;
    SigInteger, SigDouble: Signal;
Begin
  SerVostok := Connect("", "Elesy.Vostok");
  SigInteger := AddSignal(SerVostok, "AK.Sibnp.Test.Integer");
  SigDouble := AddSignal(SerVostok, "AK.Sibnp.Test.Double");
  GotoIfLess(GetValue(SigDouble), 10, LExit);
  ...
  Disconnect(SerVostok);
End.
```

Типы данных скрипта: `Signal`, `Server`, `Variant` (строки `signal`,
`server`, `variant`).

Ключевые слова (по строкам `ascii_strings.txt`):

- Управление, связь: `Connect`, `Disconnect`, `AddSignal`, `DelSignal`.
- Значения: `GetValue`, `GetValueFromCache`, `SetValue`, `SetQuality`,
  `ValueToStr`.
- Задержки/пауза: `Sleep`, `SleepTime`, `Pause`.
- Переходы: `GotoIfLess`, `GotoIfEqual`, `GotoIfGreater`, `Cancel` (метки вида
  `:LExit`).
- Дата/время: `EncodeDate`, `EncodeTime`, `DecodeDate`, `DecodeTime`,
  `AddTime`, `Trunc`.
- Прочее: `ClearLog`, `write`, `while`/`downto`, `function`/`Result`,
  `program`/`begin`/`end`.

Описание синтаксиса скриптов опирается на строки/примеры DFM и на дамп
`TFormCompiling` (компиляция скрипта в интерпретируемые команды).

### Интерпретатор скриптов: не реализован

Перезапись сохранила только редактор и сохранение/загрузку `.stx`. В `model.rs`
поле `Event::request_id` зарезервировано под синхронный хост скрипта
(«Reserved for a synchronous script host»), но интерпретатор не реализован.
UI явно сообщает: «Интерпретатор скриптов не реализован в этой версии;
редактор сохраняет текст.» Это ограничение задокументировано в README.

## Запись качества: намеренно пропущено

Стандартный OPC DA 2.0 не предусматривает записи качества значения.
Возможность «Установить качество» (`SetQuality`/`TFormSetQuality`/`tbSetQuality`)
в оригинале реализована через **проприетарный** пакет `EleSy.OPCIntercom`
(строка `EleSy.OPCIntercom` в `ascii_strings.txt`, рядом также `EleSy.DualSource`).

Дампы `dis_setq.txt`/`dis_signal_setq.txt`/`dis_quality.txt` подтверждают путь:
установка `WORD` качества (`mov dx, 0xfff5` / сравнение состояния в
`dis_quality.txt` с константами 0xC0/0), запись поля через вызов
`0x52abf8` — этот адрес находится за пределами стандартных интерфейсов OPC DA
и соответствует сигнальному объекту EleSy (в коде — обращение к полю
смещения `+0x18` и `+0x24` объекта сигнала, тип поля `VT_I2` = 0x13
в `dis_signal_setq.txt`).

Поскольку `EleSy.OPCIntercom` не является частью OPC DA и не распространяется,
перезапись намеренно не реализует запись качества (только чтение качества,
общая для DA и UA). Это отражено в README и в диалоге «О программе».

## Сводная таблица «оригинал → перезапись»

| Функция оригинала | Статус в opcda-egui |
|---|---|
| Подключение к DA-серверу (локальное/по DCOM, ProgID) | реализовано (`bff/da.rs`) |
| Перечисление серверов (OPCEnum) | реализовано (локально/удалённо) |
| Обзор адресного пространства (ветки/листья) | реализовано (DA, UA) |
| Таблица сигналов + настройка столбцов | реализовано (`columns.rs`) |
| Периодическое чтение, кэш | реализовано |
| Запись значений с выбором типа | частично, в разработке |
| Проверка связи, авто/ручное переподключение | реализовано |
| CSV-дерево, журнал | реализовано |
| Настройки (интервалы, таймауты, цвета) | реализовано (`settings.rs`) |
| Скриптовый язык (.stx) — интерпретация | **не реализовано** (только редактор) |
| Запись качества (реализована через EleSy.OPCIntercom) | **не реализовано** (намеренно) |
| Асинхронные уведомления IConnectionPoint | не реализовано (чтение синхронное) |

## Обработчики и методы

`handlers.txt` отчитывает `handlers 175`, из них сопоставлено `171 of 175`
(не сопоставлены 4 обработчика диалога «выбор компьютера»:
`ActionSelectComputerExecute`, `ActionSelectComputerUpdate`,
`TreeViewComputersDblClick`, `TreeViewComputersExpanding`).
`methods.json` — словарь `имя_метода -> адрес` (171 запись — эти
опубликованные методы/события, включая названные выше действия `Action*`,
находятся в таблице адресов). В `methods.txt` фиксируется «candidate classes
1668, mapped 0 classes» — основная масса строк с префиксом `T*` не сопоставлена
с классами, поэтому 171 — это подтверждённое подмножество опубликованных
методов, а не полный перечень.

Набор восстановленных имён методов (события и action-обработчики): `AboutClick`,
`ActionChangeExecute/Update`, `ActionColumnsExecute/Update`,
`ActionDefaultExecute/Update`, `ActionFindExecute`,
`ActionGoToLineNumberExecute/Update`, `ActionHelpExecute`,
`ActionHideJournalExecute/Update`, `ActionLoadTreeExecute/Update`,
`ActionOkExecute/Update`, `ActionOptionsScriptExecute/Update`,
`ActionPrintExecute`, `ActionSelect/Hide/Down/Up/Reflect/Default`,
`load/save` скрипта и журнала, `actDelSignalOutOfTabloExecute/Update`,
`pmSetQualityPopup`, обработчики `tvOPCSignals*`, `sgSignalTablo*`
(Click, DblClick, DragDrop, CompareCol, DrawCell, MouseDown/Up), события
формы `FormCreate/Destroy/Close/CloseQuery/Resize/Show`.

В перезаписи эти обработчики перенесены как функции/действия в
`src/app.rs`/`src/ui.rs` (подключение/отключение, добавление сигналов,
настройка столбцов, скрипт, журнал, справка/«О программе»).

## Ограничения реверса

- Точные тела многих методов не восстановлены (только адреса и сигнатуры
  по дизассемблеру); назначение ряда строк `TForm*`/`Unit*` выводится из
  контекста.
- `dis_*.txt` дают низкоуровневые фрагменты; выводы о внутреннем устройстве
  `EleSy.OPCIntercom` сделаны по доступу к полям сигнального объекта и не
  являются полной спецификацией.
- Точное соответствие всех 9 столбцов оригинального `sgSignalTablo` имени и
  порядку сверено с реализацией коридорно; гарантий пиксельной точности нет.