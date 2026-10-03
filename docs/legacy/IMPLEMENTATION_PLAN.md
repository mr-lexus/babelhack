> Архив прежней реализации Interview Translator. Не отражает текущий Babel Hack; актуальная документация — в [README](../../README.md).

# Детальный план исправления синхронизации перевода

## 1️⃣ PRIORITY 1.1: Добавить sequence numbers к Delta/Done

### Шаг 1: Обновить TranslationMsg enum

**Файл:** `src-tauri/src/translate.rs`

```rust
#[derive(Debug, Clone)]
pub enum TranslationMsg {
    Delta {
        revision: u64,
        delta: String,
        is_final: bool,
        seq: u32,  // ← Порядковый номер для этого revision (0-based)
    },
    Done {
        revision: u64,
        full_text: String,
        is_final: bool,
        seq: u32,  // ← Финальный seq (количество Deltas)
    },
}
```

### Шаг 2: Обновить translate::spawn

```rust
pub fn spawn(
    revision: u64,
    snapshot: SourceSnapshot,
    openai_key: String,
    model: String,
    speech_origin: Option<Instant>,
    delta_tx: mpsc::UnboundedSender<TranslationMsg>,
    done_tx: mpsc::UnboundedSender<u64>,
    metrics: Arc<Metrics>,
) {
    tokio::spawn(async move {
        match translate(
            &snapshot,
            revision,
            &model,
            &openai_key,
            speech_origin,
            &delta_tx,
            &metrics,
        )
        .await
        {
            Ok(_) => {}
            Err(e) => log::warn!("translation rev {revision} failed: {e:#}"),
        }
        let _ = done_tx.send(revision);
    });
}

pub(crate) async fn translate(
    snapshot: &SourceSnapshot,
    revision: u64,
    model: &str,
    api_key: &str,
    speech_origin: Option<Instant>,
    delta_tx: &mpsc::UnboundedSender<TranslationMsg>,
    metrics: &Metrics,
) -> anyhow::Result<String> {
    // ... setup code ...
    
    let mut seq = 0u32;
    let mut first = true;
    
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(delta) = chunk
            .choices
            .first()
            .and_then(|c| c.delta.content.clone())
        {
            if first {
                first = false;
                let ttft = req_start.elapsed().as_millis() as f64;
                metrics.push_ttft(ttft);
                // ... metrics logging ...
                
                // Отправить "начало" с seq=0
                let _ = delta_tx.send(TranslationMsg::Delta {
                    revision,
                    delta: String::new(),
                    is_final: snapshot.is_final,
                    seq: 0,
                });
                seq += 1;
            }
            
            full.push_str(&delta);
            let _ = delta_tx.send(TranslationMsg::Delta {
                revision,
                delta,
                is_final: snapshot.is_final,
                seq,
            });
            seq += 1;
        }
    }
    
    // ... metrics ...
    
    let _ = delta_tx.send(TranslationMsg::Done {
        revision,
        full_text: full.clone(),
        is_final: snapshot.is_final,
        seq,  // ← Финальный seq
    });
    
    Ok(full)
}
```

### Шаг 3: Обновить overlay.rs для валидации sequence

```rust
#[derive(Debug, Default)]
pub struct OverlayState {
    lines: VecDeque<String>,
    current: String,
    current_revision: u64,
    current_seq: u32,  // ← Ожидаемый seq
    expected_deltas: Option<u32>,  // ← Сколько Deltas мы ждем
}

impl OverlayState {
    pub fn on_msg(&mut self, msg: &TranslationMsg) -> OverlayAction {
        match msg {
            TranslationMsg::Delta {
                revision,
                delta,
                is_final,
                seq,
            } => {
                if *revision < self.current_revision {
                    return OverlayAction::Stale;
                }
                
                if *revision > self.current_revision {
                    // Смена revision: проверить, что получили все Deltas от предыдущей
                    if let Some(expected) = self.expected_deltas {
                        if self.current_seq != expected {
                            log::warn!(
                                "lost deltas: rev {} seq {}/{} (missing)",
                                self.current_revision,
                                self.current_seq,
                                expected
                            );
                        }
                    }
                    
                    self.current.clear();
                    self.current_revision = *revision;
                    self.current_seq = 0;
                    self.expected_deltas = None;
                }
                
                // Проверить seq для этого revision
                if *seq != self.current_seq {
                    log::warn!(
                        "seq mismatch: rev {} expected {} got {}",
                        revision, self.current_seq, seq
                    );
                    // Можно либо пропустить, либо обнулить
                    // Здесь пропускаем, чтобы избежать дублей
                    return OverlayAction::Noop;
                }
                
                self.current.push_str(delta);
                self.current_seq += 1;
                OverlayAction::Emit(self.snapshot(delta, *is_final))
            }
            TranslationMsg::Done {
                revision,
                full_text,
                is_final,
                seq,
            } => {
                if *revision < self.current_revision {
                    return OverlayAction::Stale;
                }
                if *revision != self.current_revision {
                    return OverlayAction::Noop;
                }
                
                // Запомнить, сколько Deltas мы ожидаем
                self.expected_deltas = Some(*seq);
                
                // Проверить, что seq совпадает (все Deltas получены)
                if self.current_seq != *seq {
                    log::warn!(
                        "seq count mismatch: rev {} expected {} got {}",
                        revision, self.current_seq, seq
                    );
                }
                
                if full_text.trim().is_empty() {
                    return OverlayAction::Noop;
                }
                
                if *is_final {
                    push_line(&mut self.lines, full_text);
                    self.current.clear();
                    self.current_seq = 0;
                    self.expected_deltas = None;
                    OverlayAction::Emit(self.snapshot("", true))
                } else if self.current != *full_text {
                    self.current = full_text.clone();
                    self.current_seq = *seq;  // Синхронизировать seq
                    OverlayAction::Emit(self.snapshot("", false))
                } else {
                    OverlayAction::Noop
                }
            }
        }
    }
    
    // ... остальное ...
}
```

### Шаг 4: Обновить Frontend для валидации seq

**Файл:** `src/Overlay.tsx`

```typescript
interface TranslationMessage {
  lines: string[];
  current: string;
  delta: string;
  revision: number;
  is_final: boolean;
  seq: number;  // ← Новое поле
}

export default function Overlay() {
  const [currentSeq, setCurrentSeq] = useState(0);
  const [currentRevision, setCurrentRevision] = useState(0);

  useEffect(() => {
    const un = listen<TranslationMessage>("translation", (e) => {
      const { lines: newLines, current: newCurrent, seq, revision, is_final } = e.payload;

      // Проверить seq целостность
      if (revision === currentRevision) {
        if (seq !== currentSeq) {
          console.warn(
            `seq mismatch: expected ${currentSeq} got ${seq}`
          );
        }
      } else {
        // Новый revision: сбросить seq
        console.log(`revision change: ${currentRevision} -> ${revision}`);
        setCurrentRevision(revision);
        setCurrentSeq(0);
      }

      setFinalized((prev) => {
        if (
          prev.length === newLines.length &&
          prev.every((s, i) => s === newLines[i])
        ) {
          return prev;
        }
        return newLines;
      });

      setActive(newCurrent);
      
      if (!is_final) {
        setCurrentSeq(seq + 1);
      } else {
        setCurrentSeq(0);
      }
    });

    // ... остальное ...
  }, []);

  // ... остальное ...
}
```

---

## 2️⃣ PRIORITY 1.2: Graceful shutdown с ожиданием завершения

### Файл: `src-tauri/src/lib.rs`

```rust
pub struct Session {
    audio: audio::AudioCapture,
    audio_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    metrics: Arc<Metrics>,
    handles: Vec<tokio::task::AbortHandle>,
    // ← Добавить каналы для координации завершения
    stop_tx: tokio::sync::oneshot::Sender<()>,
}

#[tauri::command]
fn stop_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(mut s) = state.session.lock().unwrap().take() {
        // Graceful stop вместо abrupt abort
        s.stop_gracefully(&app);
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
    events::emit_status(&app, "idle", None);
    Ok(())
}

impl Session {
    fn stop_gracefully(&mut self, app: &AppHandle) {
        // 1. Отправить сигнал на остановку
        let _ = self.stop_tx.send(());
        
        // 2. Дождаться завершения handlers с timeout (5 секунд)
        let rt = tokio::runtime::Handle::try_current();
        if let Ok(handle) = rt {
            let timeout = std::time::Duration::from_secs(5);
            let start = std::time::Instant::now();
            
            // Ждем, пока все handlers завершатся
            while !self.handles.is_empty() && start.elapsed() < timeout {
                self.handles.retain(|h| !h.is_finished());
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        
        // 3. Потом abort остальные
        for h in self.handles.drain(..) {
            h.abort();
        }
        
        // 4. Логировать события
        self.metrics.log_event("session_stop", serde_json::Value::Null);
        log::info!("session stopped gracefully");
    }
}
```

### Альтернатива: Channel-based graceful shutdown в coordinator.rs

```rust
pub struct Coordinator {
    pub updates_tx: mpsc::UnboundedSender<SourceSnapshot>,
    pub abort: tokio::task::AbortHandle,
    pub stop_rx: tokio::sync::oneshot::Receiver<()>,  // ← Слушать сигнал stop
}

pub fn spawn(
    openai_key: String,
    model: String,
    delta_tx: mpsc::UnboundedSender<TranslationMsg>,
    metrics: Arc<Metrics>,
    app: AppHandle,
) -> (Coordinator, tokio::sync::oneshot::Sender<()>) {
    let (updates_tx, mut updates_rx) = mpsc::unbounded_channel::<SourceSnapshot>();
    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<u64>();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
    let mut cs = CoordinatorState::new();

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(snapshot) = updates_rx.recv() => {
                    let action = cs.on_update(snapshot);
                    handle_action(...);
                }
                Some(revision) = done_rx.recv() => {
                    let action = cs.on_complete(revision);
                    handle_action(...);
                }
                _ = &mut stop_rx => {
                    // Сигнал на остановку — завершить gracefully
                    log::info!("coordinator received stop signal");
                    break;
                }
                else => break,
            }
        }
    });

    (
        Coordinator {
            updates_tx,
            abort: handle.abort_handle(),
            stop_rx,
        },
        stop_tx,
    )
}
```

---

## 3️⃣ PRIORITY 1.3: Timeout на OpenAI stream

### Файл: `src-tauri/src/translate.rs`

```rust
use std::time::Duration;

pub(crate) async fn translate(
    snapshot: &SourceSnapshot,
    revision: u64,
    model: &str,
    api_key: &str,
    speech_origin: Option<Instant>,
    delta_tx: &mpsc::UnboundedSender<TranslationMsg>,
    metrics: &Metrics,
) -> anyhow::Result<String> {
    let config = OpenAIConfig::new().with_api_key(api_key);
    let client = Client::with_config(config);

    // ... setup messages ...

    let request = CreateChatCompletionRequestArgs::default()
        .model(model)
        .messages(messages)
        .temperature(0.2_f32)
        .stream(true)
        .build()
        .map_err(|e| anyhow!("failed to build chat request: {e}"))?;

    let req_start = Instant::now();
    
    // ← ДОБАВИТЬ TIMEOUT
    let stream_result = tokio::time::timeout(
        Duration::from_secs(30),  // 30 сек на весь поток
        client
            .chat()
            .create_stream(request)
    )
    .await;
    
    let mut stream = stream_result
        .context("OpenAI stream timeout (30s)")? // timeout error message
        .context("failed to start OpenAI chat stream")?;

    let mut full = String::new();
    let mut seq = 0u32;
    let mut first = true;
    
    loop {
        // ← ДОБАВИТЬ TIMEOUT на каждый chunk
        let chunk_result = tokio::time::timeout(
            Duration::from_secs(10),  // 10 сек ожидания между chunks
            stream.next()
        )
        .await;
        
        let chunk = match chunk_result {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(e))) => return Err(anyhow!(e)),
            Ok(None) => break,  // Stream закончился
            Err(_) => {
                log::warn!("OpenAI chunk timeout (10s) for rev {}", revision);
                // Отправить Done даже если timeout
                let _ = delta_tx.send(TranslationMsg::Done {
                    revision,
                    full_text: full.clone(),
                    is_final: snapshot.is_final,
                    seq,
                });
                return Err(anyhow!("OpenAI chunk timeout (10s)"));
            }
        };

        if let Some(delta) = chunk
            .choices
            .first()
            .and_then(|c| c.delta.content.clone())
        {
            if first {
                first = false;
                let ttft = req_start.elapsed().as_millis() as f64;
                metrics.push_ttft(ttft);
                let _ = delta_tx.send(TranslationMsg::Delta {
                    revision,
                    delta: String::new(),
                    is_final: snapshot.is_final,
                    seq: 0,
                });
                seq = 1;
            }

            full.push_str(&delta);
            let _ = delta_tx.send(TranslationMsg::Delta {
                revision,
                delta,
                is_final: snapshot.is_final,
                seq,
            });
            seq += 1;
        }
    }

    let total_ms = req_start.elapsed().as_millis() as f64;
    metrics.push_total(total_ms);
    metrics.log_event("translation_done", serde_json::json!({
        "revision": revision,
        "total_ms": total_ms,
        "is_final": snapshot.is_final,
        "seq": seq,
    }));
    
    let _ = delta_tx.send(TranslationMsg::Done {
        revision,
        full_text: full.clone(),
        is_final: snapshot.is_final,
        seq,
    });
    
    Ok(full)
}
```

---

## 4️⃣ PRIORITY 2.1: Улучшить debouncing в Coordinator

### Файл: `src-tauri/src/coordinator.rs`

```rust
#[derive(Debug)]
pub struct CoordinatorState {
    revision: u64,
    active: Option<u64>,
    pending: Option<(u64, SourceSnapshot)>,
    last_started: Option<SourceSnapshot>,
    last_started_current: Option<String>,  // ← Кэшировать только `current`
}

impl CoordinatorState {
    pub fn new() -> Self {
        Self {
            revision: 0,
            active: None,
            pending: None,
            last_started: None,
            last_started_current: None,
        }
    }

    fn try_start(&mut self) -> Action {
        if self.active.is_some() {
            return Action::Coalesced;
        }
        let Some((rev, snapshot)) = self.pending.take() else {
            return Action::Idle;
        };
        if !snapshot.should_translate() {
            return Action::Coalesced;
        }
        
        // ← УЛУЧШЕННАЯ ЛОГИКА: сравниваем только `current` для interim
        if !snapshot.is_final {
            if let Some(prev_current) = &self.last_started_current {
                if prev_current == &snapshot.current {
                    return Action::Coalesced;  // Идентичный interim текст
                }
            }
        }
        // Финальный всегда переводится (может быть correction)
        
        self.last_started = Some(snapshot.clone());
        self.last_started_current = Some(snapshot.current.clone());
        self.active = Some(rev);
        
        Action::Translate {
            revision: rev,
            snapshot,
        }
    }
}

// Unit тесты
#[cfg(test)]
mod tests {
    use super::*;

    // ... existing tests ...

    #[test]
    fn identical_interim_not_resent_optimized() {
        let mut cs = CoordinatorState::new();
        let Action::Translate { revision: r1, .. } = cs.on_update(snap("Hello")) else {
            panic!();
        };
        assert_eq!(cs.on_complete(r1), Action::Idle);
        
        // Identical interim: skip
        assert_eq!(cs.on_update(snap("Hello")), Action::Coalesced);
        
        // Different interim: translate
        let Action::Translate { revision: r2, .. } = cs.on_update(snap("Hello there")) else {
            panic!("expected translate");
        };
        assert_eq!(r2, r1 + 1);
    }
}
```

---

## 5️⃣ PRIORITY 2.2: Visual feedback для смены revision

### Файл: `src/Overlay.tsx`

```typescript
interface TransitionState {
  type: 'active' | 'transitioning';
  prevText?: string;
  nextRevision?: number;
}

export default function Overlay() {
  const [finalized, setFinalized] = useState<string[]>([]);
  const [active, setActive] = useState("");
  const [transition, setTransition] = useState<TransitionState | null>(null);
  const [revision, setRevision] = useState(0);

  // ... config load ...

  useEffect(() => {
    const un = listen<TranslationPayload>("translation", (e) => {
      const { lines: newLines, current: newCurrent, revision: newRevision } = e.payload;

      setFinalized(newLines);

      // Проверить смену revision
      if (newRevision !== revision) {
        if (active) {
          // Был активный текст, но пришел новый revision
          setTransition({
            type: 'transitioning',
            prevText: active,
            nextRevision: newRevision,
          });
          
          // Показать transitioning state 300ms, потом switch
          setTimeout(() => {
            setActive(newCurrent);
            setRevision(newRevision);
            setTransition(null);
          }, 300);
        } else {
          setActive(newCurrent);
          setRevision(newRevision);
        }
      } else {
        setActive(newCurrent);
      }
    });

    // ... listen cleanup ...
  }, []);

  // Render transitioning state
  if (transition?.type === 'transitioning') {
    return (
      <div style={{ opacity: 0.5, filter: 'blur(1px)' }}>
        {/* Старый текст с эффектом fade-out */}
        <div style={{ color: historyColor, fontSize: historyFont }}>
          {transition.prevText}
        </div>
      </div>
    );
  }

  // ... остальной render code ...
}
```

---

## 6️⃣ PRIORITY 2.3: Batching Delta events

### Файл: `src-tauri/src/translate.rs` - батчевание

```rust
pub(crate) async fn translate_with_batching(
    snapshot: &SourceSnapshot,
    revision: u64,
    model: &str,
    api_key: &str,
    speech_origin: Option<Instant>,
    delta_tx: &mpsc::UnboundedSender<TranslationMsg>,
    metrics: &Metrics,
) -> anyhow::Result<String> {
    // ... setup ...
    
    let mut full = String::new();
    let mut seq = 0u32;
    let mut batch = String::new();
    let mut batch_interval = tokio::time::interval(Duration::from_millis(10));
    let mut first = true;
    
    loop {
        tokio::select! {
            // Получить chunk от OpenAI
            chunk_result = async {
                tokio::time::timeout(
                    Duration::from_secs(10),
                    stream.next()
                ).await
            } => {
                let chunk = match chunk_result {
                    Ok(Some(Ok(c))) => c,
                    Ok(Some(Err(e))) => return Err(anyhow!(e)),
                    Ok(None) => break,
                    Err(_) => return Err(anyhow!("chunk timeout")),
                };

                if let Some(delta) = chunk
                    .choices
                    .first()
                    .and_then(|c| c.delta.content.clone())
                {
                    if first {
                        first = false;
                        let ttft = req_start.elapsed().as_millis() as f64;
                        metrics.push_ttft(ttft);
                        let _ = delta_tx.send(TranslationMsg::Delta {
                            revision,
                            delta: String::new(),
                            is_final: snapshot.is_final,
                            seq: 0,
                        });
                        seq = 1;
                    }

                    full.push_str(&delta);
                    batch.push_str(&delta);  // ← Накопить в batch
                }
            }
            
            // Периодически отправлять batch
            _ = batch_interval.tick() => {
                if !batch.is_empty() {
                    let _ = delta_tx.send(TranslationMsg::Delta {
                        revision,
                        delta: batch.clone(),  // ← Отправить накопленное
                        is_final: snapshot.is_final,
                        seq,
                    });
                    batch.clear();
                    seq += 1;
                }
            }
        }
    }
    
    // Отправить остаток
    if !batch.is_empty() {
        let _ = delta_tx.send(TranslationMsg::Delta {
            revision,
            delta: batch,
            is_final: snapshot.is_final,
            seq,
        });
        seq += 1;
    }

    // ... Done message ...
    
    Ok(full)
}
```

---

## 7️⃣ PRIORITY 3.1: Sync command для восстановления состояния

### Файл: `src-tauri/src/lib.rs`

```rust
#[tauri::command]
fn get_overlay_state(state: State<'_, AppState>) -> Result<Option<TranslationPayload>, String> {
    // Вернуть текущее состояние overlay без перевода
    // Позволяет frontend синхронизировать состояние при необходимости
    
    let guard = state.session.lock().unwrap();
    if let Some(_session) = guard.as_ref() {
        // Можно добавить Mutex<LastPayload> для хранения последнего состояния
        return Ok(Some(TranslationPayload {
            lines: vec![],
            current: String::new(),
            delta: String::new(),
            revision: 0,
            is_final: false,
        }));
    }
    Ok(None)
}
```

### Файл: `src/Overlay.tsx` - периодическая синхронизация

```typescript
export default function Overlay() {
  // ... existing state ...
  
  // Периодически синхронизировать состояние (каждые 5 сек)
  useEffect(() => {
    const syncInterval = setInterval(async () => {
      try {
        const state = await invoke<TranslationPayload | null>("get_overlay_state");
        if (state) {
          // Валидировать и обновить если нужно
          console.log("sync: overlay state validated");
        }
      } catch (e) {
        console.error("sync error:", e);
      }
    }, 5000);
    
    return () => clearInterval(syncInterval);
  }, []);

  // ... rest of component ...
}
```

---

## Итоговый чеклист реализации

- [ ] 1.1 Добавить seq numbers (Delta/Done)
- [ ] 1.1 Валидировать seq в OverlayState
- [ ] 1.1 Обновить Frontend validation
- [ ] 1.2 Реализовать graceful_stop в Session
- [ ] 1.2 Добавить stop_rx в Coordinator
- [ ] 1.3 Добавить timeout на OpenAI stream
- [ ] 1.3 Добавить timeout между chunks
- [ ] 2.1 Улучшить debounce logic
- [ ] 2.1 Добавить unit тесты
- [ ] 2.2 Добавить TransitionState в Overlay
- [ ] 2.2 Визуальный эффект при смене revision
- [ ] 2.3 Реализовать batching Delta events
- [ ] 2.3 Настроить интервал батчевания
- [ ] 3.1 Добавить get_overlay_state command
- [ ] 3.1 Периодическая синхронизация в Frontend
- [ ] Тестирование всех сценариев
- [ ] Stress тесты с быстрой речью
