use crate::{runtime::Runtime, transcript::SourceSnapshot, translate};
use std::{future::Future, sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};

pub struct Job {
    pub id: u64,
    pub snapshot: SourceSnapshot,
}

pub enum Work {
    Final(Job),
    Preview(SourceSnapshot),
}

// One owned request at a time. Finals are FIFO and interrupt speculative work.
// watch retains only the newest hypothesis: no queue of obsolete interim text.
async fn drive<F, Fut>(
    mut rx: mpsc::Receiver<Job>,
    mut previews: watch::Receiver<Option<SourceSnapshot>>,
    mut process: F,
) where
    F: FnMut(Work) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut pending = None;
    let mut last_preview = None;
    let mut next_preview = tokio::time::Instant::now();
    let mut preview_open = true;
    loop {
        if let Some(job) = pending.take().or_else(|| rx.try_recv().ok()) {
            process(Work::Final(job)).await;
            last_preview = None;
            continue;
        }
        let latest = previews.borrow_and_update().clone();
        if latest != last_preview && latest.as_ref().is_some_and(preview_ready) {
            tokio::select! {
                biased;
                job = rx.recv() => { match job { Some(j) => pending = Some(j), None => break }; continue; }
                _ = tokio::time::sleep_until(next_preview) => {}
            }
            // Re-read after throttling, since hypotheses may have changed.
            let latest = previews.borrow_and_update().clone();
            if let Some(snapshot) = latest.as_ref().filter(|s| preview_ready(s)) {
                next_preview = tokio::time::Instant::now() + Duration::from_millis(750);
                let request = process(Work::Preview(snapshot.clone()));
                tokio::pin!(request);
                loop {
                    tokio::select! {
                        biased;
                        job = rx.recv(), if pending.is_none() => {
                            match job {
                                Some(j) => {
                                    let promote = same_source(&j.snapshot, snapshot);
                                    pending = Some(j);
                                    // A matching final adopts this request. Its
                                    // result enters the cache and is committed next.
                                    if !promote { break; }
                                }
                                None => return,
                            }
                        }
                        changed = previews.changed(), if preview_open => {
                            if changed.is_err() { preview_open = false; }
                            let newer = previews.borrow_and_update().clone();
                            // Growth can finish its useful prefix; a correction or
                            // a new clause invalidates the owned request immediately.
                            if pending.is_none() && !newer.as_ref().is_some_and(|n| n.committed == snapshot.committed && n.current.starts_with(&snapshot.current)) { break; }
                        }
                        _ = &mut request => break,
                    }
                }
                last_preview = latest;
            }
        } else {
            tokio::select! {
                biased;
                job = rx.recv() => { match job { Some(j) => pending = Some(j), None => break } }
                changed = previews.changed(), if preview_open => {
                    if changed.is_err() { preview_open = false; }
                }
            }
        }
    }
}

fn preview_ready(s: &SourceSnapshot) -> bool {
    s.current.split_whitespace().count() >= 3 || s.current.chars().count() >= 16
}

fn same_source(a: &SourceSnapshot, b: &SourceSnapshot) -> bool {
    a.current == b.current && a.committed == b.committed
}

pub async fn run(
    rx: mpsc::Receiver<Job>,
    previews: watch::Receiver<Option<SourceSnapshot>>,
    key: String,
    rt: Arc<Runtime>,
) {
    // Reuse the HTTPS connection across short clauses.
    let client = translate::client(key);
    let cache = std::sync::Mutex::new(None::<(SourceSnapshot, String)>);
    drive(rx, previews, |work| {
        let rt = &rt;
        let client = &client;
        let cache = &cache;
        async move {
            let (id, snapshot) = match work {
                Work::Final(job) => (Some(job.id), job.snapshot),
                Work::Preview(s) => (None, s),
            };
            if let Some(id) = id {
                let cached = cache.lock().unwrap().clone();
                if let Some((_, text)) = cached.filter(|(s, _)| same_source(s, &snapshot)) {
                    rt.complete(id, Ok(text));
                    return;
                }
            }
            let result = request_with_retry(id.is_some(), || {
                translate::translate(&snapshot, client, rt, id.is_none())
            })
            .await
            .map_err(|e| format!("{e:#}"));
            if let Some(id) = id {
                rt.complete(id, result);
                *cache.lock().unwrap() = None;
            } else if let Ok(text) = result {
                *cache.lock().unwrap() = Some((snapshot, text));
            }
            // Speculative errors don't replace useful captions with an error.
            // A final request still reports its failure and preserves the source.
        }
    })
    .await;
}

// One bounded retry for transient failures, never an unbounded SDK retry loop
// holding the FIFO. Dropping this future cancels both the request and retry.
async fn request_with_retry<F, Fut>(final_request: bool, mut request: F) -> anyhow::Result<String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<String>>,
{
    for attempt in 0..2 {
        let result = match tokio::time::timeout(
            Duration::from_secs(if final_request { 10 } else { 5 }),
            request(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                Err(anyhow::Error::new(e).context("Перевод задерживается: превышен лимит запроса"))
            }
        };
        match result {
            Err(e) if final_request && attempt == 0 && translate::retryable(&e) => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            other => return other,
        }
    }
    unreachable!()
}

#[cfg(test)]
async fn process_jobs<F, Fut>(rx: mpsc::Receiver<Job>, mut process: F)
where
    F: FnMut(Job) -> Fut,
    Fut: Future<Output = ()>,
{
    let (_tx, previews) = watch::channel(None);
    drive(rx, previews, |work| {
        process(match work {
            Work::Final(j) => j,
            _ => unreachable!(),
        })
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };
    fn job(id: u64) -> Job {
        Job {
            id,
            snapshot: SourceSnapshot {
                committed: String::new(),
                current: "Repeated sentence.".into(),
                is_final: true,
                word_start: None,
            },
        }
    }

    #[tokio::test]
    async fn transient_final_retries_once_but_preview_and_permanent_errors_do_not() {
        use std::sync::atomic::AtomicUsize;
        let calls = AtomicUsize::new(0);
        let result = request_with_retry(true, || async {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                tokio::time::timeout(Duration::ZERO, std::future::pending::<()>()).await?;
            }
            Ok("Complete sentence".into())
        })
        .await
        .unwrap();
        assert_eq!(result, "Complete sentence");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        for final_request in [false, true] {
            calls.store(0, Ordering::SeqCst);
            let _ = request_with_retry(final_request, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(anyhow::anyhow!("Invalid model"))
            })
            .await;
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
        calls.store(0, Ordering::SeqCst);
        let _ = request_with_retry(false, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::timeout(Duration::ZERO, std::future::pending::<()>()).await?;
            Ok(String::new())
        })
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    #[ignore = "synthetic fragmented speech replay using saved OpenAI key; no audio capture"]
    async fn live_coherent_sentence_replay() {
        use crate::transcript::TranscriptReconciler;
        let cfg = crate::config::Config {
            source_language: "en".into(),
            target_language: "ru".into(),
            glossary: String::new(),
            ..crate::config::load()
        };
        let client =
            translate::client(crate::secure::get_openai_key().expect("OpenAI key required"));
        let metrics = crate::metrics::Metrics::new();
        let (tx, rx) = mpsc::channel(8);
        let (preview_tx, preview_rx) = watch::channel(None);
        let events = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let seen = events.clone();
        let start = std::time::Instant::now();
        let worker = tokio::spawn(async move {
            let visible = Mutex::new(String::new());
            drive(rx, preview_rx, |work| {
                let cfg = &cfg;
                let client = &client;
                let metrics = &metrics;
                let seen = &seen;
                let visible = &visible;
                async move {
                    let (id, snapshot) = match work {
                        Work::Preview(s) => (None, s),
                        Work::Final(j) => (Some(j.id), j.snapshot),
                    };
                    let hint = visible.lock().unwrap().clone();
                    let result = request_with_retry(id.is_some(), || {
                        translate::stream_translation_with_draft(
                            &snapshot,
                            client,
                            cfg,
                            metrics,
                            &hint,
                            |raw, complete| {
                                let text = if id.is_none() {
                                    translate::mask_unstable_tail(raw)
                                } else {
                                    raw
                                };
                                let mut old = visible.lock().unwrap();
                                if translate::should_present(&old, text, complete) {
                                    *old = text.into();
                                    seen.lock().unwrap().push(serde_json::json!({
                                    "ms":start.elapsed().as_millis(), "kind":"draft", "text":text,
                                    "source":snapshot.current, "provisional":id.is_none(),
                                }));
                                }
                            },
                        )
                    })
                    .await
                    .expect("Synthetic translation failed");
                    if let Some(id) = id {
                        visible.lock().unwrap().clear();
                        seen.lock().unwrap().push(serde_json::json!({
                            "ms":start.elapsed().as_millis(), "kind":"final", "id":id,
                            "source":snapshot.current, "text":result,
                        }));
                    }
                }
            })
            .await;
        });
        let mut r = TranscriptReconciler::new();
        let parts = [
            "Not only can we increase the strength of these deep sleep brain",
            "waves,",
            "but we can also nearly double the memory",
            "benefit",
            "that people get from sleep.",
        ];
        for part in parts {
            r.update(part, false, false);
            preview_tx.send_replace(r.preview());
            for _ in 0..19 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                assert!(
                    r.flush_due().is_none(),
                    "Timer must not split an ongoing sentence"
                );
            }
            let snapshot = r.update(part, true, true).unwrap();
            assert!(
                !snapshot.is_final,
                "ASR fragment must not be a final caption"
            );
            preview_tx.send_replace(r.preview());
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
        let source = r.flush_due().unwrap();
        assert_eq!(source.current, parts.join(" "));
        let sealed_ms = start.elapsed().as_millis();
        tx.send(Job {
            id: 1,
            snapshot: source,
        })
        .await
        .unwrap();
        preview_tx.send_replace(None);
        drop(tx);
        drop(preview_tx);
        worker.await.unwrap();
        let events = events.lock().unwrap();
        let finals: Vec<_> = events.iter().filter(|e| e["kind"] == "final").collect();
        assert_eq!(
            finals.len(),
            1,
            "One whole sentence, not five caption fragments"
        );
        assert!(finals[0]["text"]
            .as_str()
            .unwrap()
            .chars()
            .any(|c| ('а'..='я').contains(&c)));
        let first = events
            .iter()
            .find(|e| e["kind"] == "draft")
            .expect("A visible interim translation is required");
        assert!((first["ms"].as_u64().unwrap() as u128) < sealed_ms);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../artifacts/coherent-replay.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec_pretty(&*events).unwrap()).unwrap();
        println!("Coherent replay: first draft {} ms, sentence sealed {sealed_ms} ms, final {} ms; translation: {}",
            first["ms"], finals[0]["ms"], finals[0]["text"]);
    }
    #[tokio::test]
    #[ignore = "uses saved OpenAI key with synthetic timed hypotheses; never captures audio"]
    async fn live_simultaneous_translation_precedes_speech_final() {
        let client =
            translate::client(crate::secure::get_openai_key().expect("OpenAI key required"));
        let cfg = crate::config::Config {
            source_language: "en".into(),
            target_language: "ru".into(),
            glossary: String::new(),
            ..crate::config::load()
        };
        let metrics = crate::metrics::Metrics::new();
        let (tx, rx) = mpsc::channel(8);
        let (preview_tx, preview_rx) = watch::channel(None);
        let started = std::time::Instant::now();
        let first_visible = Arc::new(Mutex::new(None));
        let finals = Arc::new(Mutex::new(vec![]));
        let first = first_visible.clone();
        let completed = finals.clone();
        let worker = tokio::spawn(async move {
            drive(rx, preview_rx, |work| {
                let client = &client;
                let cfg = &cfg;
                let metrics = &metrics;
                let first = &first;
                let completed = &completed;
                async move {
                    let (id, source) = match work {
                        Work::Final(j) => (Some(j.id), j.snapshot),
                        Work::Preview(s) => (None, s),
                    };
                    let result = tokio::time::timeout(
                        Duration::from_secs(12),
                        translate::stream_translation(
                            &source,
                            client,
                            cfg,
                            metrics,
                            |text, _complete| {
                                if id.is_none() && text.chars().any(|c| ('а'..='я').contains(&c))
                                {
                                    first
                                        .lock()
                                        .unwrap()
                                        .get_or_insert(started.elapsed().as_millis());
                                }
                            },
                        ),
                    )
                    .await
                    .expect("Translation timeout")
                    .expect("Translation failed");
                    if let Some(id) = id {
                        completed.lock().unwrap().push((id, result));
                    }
                }
            })
            .await;
        });
        let mut source = job(1).snapshot;
        source.is_final = false;
        source.current = "I lead a team of software engineers".into();
        preview_tx.send_replace(Some(source.clone()));
        tokio::time::sleep(Duration::from_secs(3)).await;
        source.current.push_str(" and we build reliable products");
        preview_tx.send_replace(Some(source.clone()));
        tokio::time::sleep(Duration::from_secs(3)).await;
        let speech_final_ms = started.elapsed().as_millis();
        preview_tx.send_replace(None);
        source.is_final = true;
        tx.send(Job {
            id: 1,
            snapshot: source,
        })
        .await
        .unwrap();
        drop(tx);
        drop(preview_tx);
        worker.await.unwrap();
        let first = first_visible
            .lock()
            .unwrap()
            .expect("Must emit interim translation before final speech");
        assert!(first < speech_final_ms);
        assert_eq!(finals.lock().unwrap().len(), 1);
        println!("Synthetic streaming replay: first readable Russian = {first} ms; speech_final = {speech_final_ms} ms; final translations = 1");
    }
    #[tokio::test]
    async fn finals_preempt_preview_and_stale_hypotheses_never_queue() {
        let (tx, rx) = mpsc::channel(8);
        let (preview_tx, preview_rx) = watch::channel(None);
        let (seen_tx, mut seen_rx) = mpsc::unbounded_channel();
        let dropped = Arc::new(AtomicBool::new(false));
        let flag = dropped.clone();
        struct DropFlag(Arc<AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let worker = tokio::spawn(drive(rx, preview_rx, move |work| {
            let seen = seen_tx.clone();
            let flag = flag.clone();
            async move {
                match work {
                    Work::Preview(s) => {
                        let _guard = DropFlag(flag);
                        seen.send(s.current).unwrap();
                        std::future::pending::<()>().await;
                    }
                    Work::Final(j) => {
                        seen.send(format!("final-{}", j.id)).unwrap();
                    }
                }
            }
        }));
        let mut preview = job(0).snapshot;
        preview.is_final = false;
        preview.current = "We discuss the system".into();
        preview_tx.send_replace(Some(preview.clone()));
        assert_eq!(seen_rx.recv().await.unwrap(), preview.current);
        for n in 0..50 {
            preview.current = format!("We discuss the system {n}");
            preview_tx.send_replace(Some(preview.clone()));
        }
        tx.send(job(1)).await.unwrap();
        tx.send(job(2)).await.unwrap();
        preview_tx.send_replace(None);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), seen_rx.recv())
                .await
                .unwrap()
                .unwrap(),
            "final-1"
        );
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(seen_rx.recv().await.unwrap(), "final-2");
        drop(tx);
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn matching_final_adopts_inflight_preview_instead_of_restarting() {
        let (tx, rx) = mpsc::channel(4);
        let (preview_tx, preview_rx) = watch::channel(None);
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let gate = release.clone();
        let completed = Arc::new(AtomicBool::new(false));
        let flag = completed.clone();
        let worker = tokio::spawn(drive(rx, preview_rx, move |work| {
            let started = started_tx.clone();
            let gate = gate.clone();
            let flag = flag.clone();
            async move {
                match work {
                    Work::Preview(_) => {
                        started.send(()).unwrap();
                        gate.notified().await;
                        flag.store(true, Ordering::SeqCst);
                    }
                    Work::Final(_) => {
                        assert!(
                            flag.load(Ordering::SeqCst),
                            "The exact same translation must finish before committing final"
                        );
                    }
                }
            }
        }));
        let mut p = job(1).snapshot;
        p.is_final = false;
        preview_tx.send_replace(Some(p));
        started_rx.recv().await.unwrap();
        tx.send(job(1)).await.unwrap();
        preview_tx.send_replace(None);
        tokio::task::yield_now().await;
        release.notify_one();
        drop(tx);
        worker.await.unwrap();
        assert!(completed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn corrected_interim_cancels_owned_request() {
        let (tx, rx) = mpsc::channel(2);
        let (preview_tx, preview_rx) = watch::channel(None);
        let (seen_tx, mut seen_rx) = mpsc::unbounded_channel();
        let worker = tokio::spawn(drive(rx, preview_rx, move |work| {
            let seen = seen_tx.clone();
            async move {
                if let Work::Preview(s) = work {
                    seen.send(s.current).unwrap();
                    std::future::pending::<()>().await;
                }
            }
        }));
        let mut p = job(0).snapshot;
        p.is_final = false;
        p.current = "I can approve this".into();
        preview_tx.send_replace(Some(p.clone()));
        assert_eq!(seen_rx.recv().await.unwrap(), p.current);
        p.current = "I cannot approve this".into();
        preview_tx.send_replace(Some(p.clone()));
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), seen_rx.recv())
                .await
                .unwrap()
                .unwrap(),
            p.current
        );
        drop(tx);
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn queued_finals_are_never_coalesced_or_deduplicated() {
        let (tx, rx) = mpsc::channel(8);
        for id in 1..=5 {
            tx.send(job(id)).await.unwrap();
        }
        drop(tx);
        let seen = Arc::new(Mutex::new(vec![]));
        process_jobs(rx, |j| {
            let seen = seen.clone();
            async move {
                tokio::task::yield_now().await;
                seen.lock().unwrap().push(j.id);
            }
        })
        .await;
        assert_eq!(*seen.lock().unwrap(), vec![1, 2, 3, 4, 5]);
    }
    #[tokio::test]
    async fn cancelling_worker_drops_inflight_request() {
        struct Probe(Arc<AtomicBool>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let (tx, rx) = mpsc::channel(1);
        tx.send(job(1)).await.unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        let flag = dropped.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let mut started = Some(started_tx);
        let worker = tokio::spawn(async move {
            process_jobs(rx, |_| {
                let probe = Probe(flag.clone());
                let started = started.take();
                async move {
                    let _probe = probe;
                    if let Some(started) = started {
                        let _ = started.send(());
                    }
                    std::future::pending::<()>().await;
                }
            })
            .await;
        });
        started_rx.await.unwrap();
        worker.abort();
        let _ = worker.await;
        assert!(dropped.load(Ordering::SeqCst));
    }
}
