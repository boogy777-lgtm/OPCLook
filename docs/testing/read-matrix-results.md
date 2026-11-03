# R2.TEST — результаты matrix и границы

Рабочая база `9f2c321` с исправлением G-R2 `5847d1a`. Данные ниже — локальные
измерения Linux настоящего `opclook-engine`, contract и backend crate; при
тестировании `test_skip_dispatch` подменяет отправку на границе Gateway. Ответы
и время подаются тестом, реальных OPC I/O или Windows SDK здесь нет. Формулы
не реализованы повторно: ожидаемые `0/1` читаются из соответствующих CSV.

- **D:** все 128 строк прочитаны и проверены через Connect, Add, optional
  accepted Write и/или Refresh, `due`; 104 случая дали реальный
  `send_due_read` и terminal, 24 не дали Read. Варианты grace/poll задаются
  контролируемым `Instant`; поля, не применимые к сочетанию N/W, не означают
  дополнительную наблюдаемую историю.
- **G:** все 128 строк разобраны; 8 сочетаний C=E=K=A=1 реально достигнуты
  через Engine с различными I/B/D. Остальные проверены как ранние отказы:
  32 disconnected, 16 invalidated epoch, 8 без зарегистрированных ключей,
  64 UnsupportedFreshness на legacy DA/Linux. Другие биты в этих строках
  не выдаются за одновременно достижимое состояние. При Windows native DA
  поддержка FreshSource меняется; соответствующие 64 случая тест явно считает
  `capability unavailable` для этого fixture, а не Unsupported PASS.
- **G-R2:** Disconnect отправлен, ответ ещё не пришёл; тест подтвердил до
  исправления ошибочный Read без Connect epoch, после исправления — отсутствие
  Read и нового permit и один terminal Cancelled для Initial+Refresh.
- **S04/E:** два accepted Write до dispatch дают одну coalesced потребность;
  третий после dispatch остаётся отдельно, subscription push и старый Read
  её не завершают. Измерены два разных Read request/terminal. Отдельные
  Write IDs для coalesced pre-dispatch needs в существующем API отсутствуют.
- **S identity:** после Disconnect и нового Connect late reply старого Read
  отклоняется, новый request остаётся pending и получает свой terminal. При
  Remove in-flight key получает один Cancelled(Removed), а поздний reply не
  создаёт второй terminal и не восстанавливает регистрацию.
- **Drop:** два actual Engine tests проверяют отмену in-flight и последующей
  pending потребности при Drop, а также отсутствие повторного callback после
  explicit shutdown. До исправления implicit Drop не вызывал cancellation.
  Sink выполняется синхронно, поэтому его существующий контракт требует
  быстрого возврата.
- **Fairness:** до исправления непрерывные Browse(A) удерживали два pending
  Refresh восемь итераций, тогда как B завершала Read. После исправления тот же
  тест получает типизированный ReadPending до отправки следующего Browse и
  выдаёт Read; обычный Poll не блокирует Browse бессрочно. UI сохраняет
  отложенную ветку в существующей очереди и повторяет после Read reply.
- **Partial:** actual Engine terminal различает возвращённый ключ и отсутствующий
  в частичном ответе ключ; поздние replies не создают повторный terminal.

Полный локальный Cargo engine suite: **88/88 passed**. Локальный App
library binary: **264/264 passed**, включая **6/6** AddChildren; новый
CLI entrypoint test: **1/1 passed**.

Windows i686, real backend transport, полное покрытие E/S
matrix не покрываются этим измерением. Числа относятся к указанной локальной
базе, а не к более раннему опубликованному CI head.
