> Архив прежней реализации Interview Translator. Не отражает текущий Babel Hack; актуальная документация — в [README](../../README.md).

# Анализ логики синхронного перевода и оверлея

## Архитектура потока данных

```
Audio (WASAPI) 
  → AudioCapture (16kHz mono)
    → Deepgram STT (async WebSocket)
      → TranscriptReconciler (группирует в предложения)
        → Coordinator (latest-wins scheduler)
          → translate::spawn (OpenAI streaming)
            → OverlayState (накопление + финализация)
              → Tauri event "translation"
                → React Overlay.tsx (render)
```

---

## 🔴 Выявленные недостатки

### 1. **Race condition: Delta vs Done мессажи не синхронизированы**
**Где:** `translate.rs` (spawn отправляет Delta, потом Done)  
**Проблема:**
- Если между Delta и Done приходит новый revision, старый текст теряется
- Нет гарантии порядка доставки (особенно если один канал отстает)
- Frontend может получить Done перед всеми Delta для этого revision

**Сценарий:**
```
Rev 1: Delta("Hello") → Done(revision=1, "Hello")
Rev 2: Delta("Hi") → [потеря Rev1::Done] → Done(revision=2, "Hi")
Frontend видит: Rev 2::Done раньше чем все Rev 1::Delta
```

---

### 2. **Потеря текста при смене revision в OverlayState**
**Где:** `overlay.rs`, метод `on_msg` при `*revision > self.current_revision`  
**Проблема:**
```rust
if *revision > self.current_revision {
    self.current.clear();  // ← ПРОСТО УДАЛЯЕМ старый текст!
    self.current_revision = *revision;
}
```
- При быстрой смене revisions промежуточный текст исчезает без визуального feedback
- Юзер видит "дергание" — текст был, потом исчез

**Пример:**
```
Rev 1: "Можете ли вы рассказать"  [display]
Rev 2 приходит → текст FULL CLEAR → пусто
Rev 2: Delta("Расскажите о своем опыте") [display]
```

---

### 3. **Отсутствие синхронизации состояния при потере события**
**Где:** Tauri event channel между backend и frontend  
**Проблема:**
- Если event "translation" потеряется (редко, но возможно), frontend и backend разойдутся
- Нет механизма "sync state" — повторной отправки snapshot
- Frontend будет ждать `current` поля, которое никогда не придет

**Последствие:**
- Overlay зависнет на старом тексте
- Пользователь не заметит, что происходит что-то не так

---

### 4. **Недостаточная дебаунцировка в Coordinator**
**Где:** `coordinator.rs`, условие в `try_start()`
```rust
if self.last_started.as_ref() == Some(&snapshot) && !snapshot.is_final {
    return Action::Coalesced;
}
```
**Проблема:**
- Проверка ищет ПОЛНОЕ совпадение snapshot (committed + current + is_final)
- При микроизменениях текста (добавление одного слова) - это новый snapshot!
- Может привести к цепочке мелких переводов вместо слияния в один

**Сценарий:**
```
STT: "Кан" → Translate(rev 1)
STT: "Канд" → Translate(rev 2)  [не слилась!]
STT: "Канди" → Translate(rev 3)  [не слилась!]
```

---

### 5. **Потеря последних событий при stop_session**
**Где:** `lib.rs`, функция `stop_session()`
```rust
pub fn stop_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(mut s) = state.session.lock().unwrap().take() {
        s.stop();  // ← Тут abort всех handles
    }
    ...
}
```
**Проблема:**
- `.abort()` на обработчиках может прервать in-flight операции
- Если translate отправил Delta но не успел отправить Done, они потеряются
- Нет graceful shutdown с ожиданием завершения

**Последствие:**
- Финальный перевод может остаться в "incomplete" состоянии
- Overlay не обновится корректно

---

### 6. **Нет timeout для незавершенных потоков перевода**
**Где:** `translate.rs`, функция `translate()`
```rust
let mut stream = client
    .chat()
    .create_stream(request)
    .await
    .context("failed to start OpenAI chat stream")?;
// ← Может зависнуть здесь, если OpenAI не ответит
while let Some(chunk) = stream.next().await {
    ...
}
```
**Проблема:**
- Нет timeout на OpenAI запрос
- Если сеть нестабильна, поток может ждать бесконечно
- Это блокирует следующую translation (у coordinator active будет занят)

---

### 7. **Нет visual feedback при смене контекста (revision)**
**Где:** Frontend (Overlay.tsx)  
**Проблема:**
- Когда Backend переходит на новый revision (новое предложение):
  - `current` очищается
  - Но пользователь видит просто "…" (пустая строка)
  - Нет информации, что это смена контекста

---

### 8. **Производительность: Delta flood**
**Где:** `translate.rs`, каждый token = отдельный event
```rust
let _ = delta_tx.send(TranslationMsg::Delta {
    revision,
    delta,
    is_final: snapshot.is_final,
});
```
**Проблема:**
- При быстром переводе (20+ tokens в секунду):
  - 20+ событий/сек через Tauri
  - 20+ re-renders в React
  - Может привести к lag на слабых машинах

---

### 9. **Потеря состояния при переподключении Deepgram**
**Где:** `dg.rs`, функция `run()`
```rust
Err(e) => {
    log::warn!("deepgram session error: {e:#}");
    on_status("error", Some(format!("Deepgram: {e}")));
    while audio_rx.try_recv().is_ok() {}  // ← Очищаем буфер!
    tokio::time::sleep(Duration::from_secs(2)).await;
}
```
**Проблема:**
- При разрыве соединения Deepgram весь audio буфер очищается
- Текущий TranscriptReconciler состояние остается, но audio потеряна
- Может привести к несогласованности между committed и audio

---

### 10. **Отсутствие дедупликации в OverlayState при Done**
**Где:** `overlay.rs`, обработка TranslationMsg::Done
```rust
} else if self.current != *full_text {
    self.current = full_text.clone();
    OverlayAction::Emit(self.snapshot("", false))
} else {
    OverlayAction::Noop
}
```
**Проблема:**
- Если Done приходит дважды с одинаковым `full_text` (при retry), это будет Noop
- Но если в frontend потеряется это событие, он не получит информацию о финализации

---

### 11. **Нет обработки очень больших текстов**
**Где:** `overlay.rs`
```rust
const MAX_LINE_CHARS: usize = 300;
```
**Проблема:**
- Если перевод превысит 300 символов, он просто обрезается
- Обрезка видна пользователю, но нет информации, что текст обрезан

---

## 📋 План исправления (приоритеты)

### **Приоритет 1: Критические (могут привести к потере данных)**

#### 1.1 Добавить transaction ID для Delta/Done паралей
```rust
pub enum TranslationMsg {
    Delta { 
        revision: u64,
        delta: String,
        is_final: bool,
        seq: u32,  // ← Порядковый номер
    },
    Done { 
        revision: u64,
        full_text: String,
        is_final: bool,
        seq: u32,  // ← Финальный номер этого revision
    },
}
```
Frontend может проверить, что получил все Delta [0..seq-1] перед обработкой Done.

#### 1.2 Graceful shutdown с ожиданием завершения
```rust
pub async fn graceful_stop(&mut self, timeout: Duration) {
    // 1. Отправить сигнал на остановку
    // 2. Дождаться завершения всех active translation с timeout
    // 3. Потом abort остальные handles
}
```

#### 1.3 Добавить timeout на OpenAI stream
```rust
let stream = tokio::time::timeout(
    Duration::from_secs(30),
    client.chat().create_stream(request)
).await?;
```

---

### **Приоритет 2: Высокий (влияют на UX)**

#### 2.1 Пересмотреть дебаунцировку в Coordinator
Вместо сравнения всего snapshot, сравнивать только `current`:
```rust
if let Some(prev) = &self.last_started {
    if prev.current == snapshot.current && !snapshot.is_final {
        return Action::Coalesced;
    }
}
```

#### 2.2 Добавить transition state для смены revision
```rust
pub enum OverlaySnapshot {
    Active { text: String, delta: String },
    Transitioning { prev_text: String, next_index: u64 },  // ← Новое
    Finalized { lines: Vec<String> },
}
```
Показать юзеру визуальное изменение при смене контекста.

#### 2.3 Батчевание Delta событий (debounce)
Вместо отправки каждого token отдельно:
```rust
// Накопить несколько tokens за 10ms, потом отправить один event
```

---

### **Приоритет 3: Средний (улучшение надежности)**

#### 3.1 Sync command для восстановления состояния
```rust
#[tauri::command]
async fn sync_overlay_state(app: AppHandle, state: State<'_, AppState>) -> Result<TranslationPayload> {
    // Вернуть текущее состояние overlay
}
```
Frontend может периодически синхронизировать состояние.

#### 3.2 Обработка большого текста
```rust
const MAX_LINE_CHARS: usize = 300;
if text.len() > MAX_LINE_CHARS {
    return format!("{}…", &text[..MAX_LINE_CHARS]);
}
```

#### 3.3 Log и metrics для Race condition обнаружения
```rust
pub fn track_delta_sequence(rev: u64, seq: u32) {
    // Логировать пропущенные seq номера
}
```

---

### **Приоритет 4: Низкий (оптимизация)**

#### 4.1 Кэширование в OverlayState
Помнить последние 3 revision, чтобы быстро вернуться назад при skip.

#### 4.2 Оптимизация TranscriptReconciler
Использовать Rope или similar для быстрого append к committed.

---

## 🎯 Рекомендуемая последовательность реализации

1. **Week 1**: Приоритет 1.1 (sequence numbers) + 1.2 (graceful shutdown)
2. **Week 2**: Приоритет 1.3 (timeout) + 2.1 (debounce fix) + 2.2 (transition visual)
3. **Week 3**: Приоритет 3.1 (sync) + 3.2 (text truncation) + 3.3 (metrics)
4. **Week 4+**: Приоритет 4 (оптимизация) по мере необходимости

---

## Тестирование

Для каждого исправления нужно добавить тесты:
- Unit тесты в `coordinator.rs` ✅ (уже есть)
- Unit тесты в `overlay.rs` ⚠️ (недостаточно)
- Integration тесты для race condition симуляции
- Stress тесты с быстрой речью (10+ tokens/sec)
- Chaos тесты: имитация потери events, delays, Deepgram disconnects
