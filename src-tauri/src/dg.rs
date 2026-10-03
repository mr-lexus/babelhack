use crate::{
    coordinator::Job,
    runtime::Runtime,
    transcript::{SourceSnapshot, TranscriptReconciler},
};
use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Response {
    Results {
        channel: Channel,
        #[serde(default)]
        is_final: bool,
        #[serde(default)]
        speech_final: bool,
        #[serde(default)]
        from_finalize: bool,
    },
    UtteranceEnd,
    Error {
        #[serde(default)]
        description: String,
    },
    #[serde(other)]
    Other,
}
#[derive(Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}
#[derive(Deserialize)]
struct Alternative {
    transcript: String,
}

fn stream_url(cfg: &crate::config::Config) -> String {
    let mut url = url::Url::parse("wss://api.deepgram.com/v1/listen").expect("static URL");
    url.query_pairs_mut().extend_pairs([
        ("model", cfg.deepgram_model.as_str()),
        ("language", cfg.source_language.as_str()),
        ("encoding", "linear16"),
        ("sample_rate", "16000"),
        ("channels", "1"),
        ("interim_results", "true"),
        ("punctuate", "true"),
        ("smart_format", "true"),
        ("endpointing", "300"),
        ("utterance_end_ms", "1000"),
    ]);
    url.into()
}

#[cfg(test)]
fn finalize_due(elapsed: Duration, has_interim: bool, stopping: bool) -> bool {
    !stopping && has_interim && elapsed >= Duration::from_millis(2400)
}

pub async fn run(
    key: String,
    mut audio_rx: mpsc::Receiver<Vec<u8>>,
    jobs: mpsc::Sender<Job>,
    previews: watch::Sender<Option<SourceSnapshot>>,
    rt: Arc<Runtime>,
    mut stop: watch::Receiver<bool>,
) {
    let mut failures = 0u32;
    loop {
        if *stop.borrow() {
            break;
        }
        let since = tokio::time::Instant::now();
        let result = run_session(&key, &mut audio_rx, &jobs, &previews, &rt, &mut stop).await;
        if *stop.borrow() {
            break;
        }
        match result {
            Ok(()) => break,
            Err(e) => {
                if since.elapsed() > Duration::from_secs(60) {
                    failures = 0;
                }
                failures += 1;
                let text = format!("{e:#}");
                if ["401", "403", "400", "402", "Очередь"]
                    .iter()
                    .any(|c| text.contains(c))
                    || failures >= 8
                {
                    rt.paused.store(true, Ordering::Relaxed);
                    rt.status(
                        "error",
                        Some(format!(
                            "Deepgram: {text}. Остановите сессию и проверьте настройки."
                        )),
                    );
                    break;
                }
                let delay = (1u64 << failures.min(4)).min(16);
                rt.status("reconnecting", Some(format!("Соединение потеряно. Повтор через {delay} с. Звук за время разрыва не восстанавливается.")));
                while audio_rx.try_recv().is_ok() {}
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(delay)) => {},
                    _ = stop.changed() => break,
                }
                while audio_rx.try_recv().is_ok() {}
            }
        }
    }
}

fn publish(snapshot: SourceSnapshot, jobs: &mpsc::Sender<Job>, rt: &Runtime) -> anyhow::Result<()> {
    rt.update(|v| v.source = snapshot.current.clone());
    if !snapshot.is_final {
        return Ok(());
    }
    let id = rt.add_source(&snapshot.current);
    if let Err(e) = jobs.try_send(Job { id, snapshot }) {
        rt.complete(
            id,
            Err("Очередь перевода переполнена. Исходный текст сохранён.".into()),
        );
        return Err(anyhow!("Очередь перевода недоступна: {e}"));
    }
    if id >= 10000 {
        return Err(anyhow!(
            "Очередь: достигнут лимит 10 000 фраз. Начните новую сессию."
        ));
    }
    Ok(())
}

async fn run_session(
    key: &str,
    audio_rx: &mut mpsc::Receiver<Vec<u8>>,
    jobs: &mpsc::Sender<Job>,
    previews: &watch::Sender<Option<SourceSnapshot>>,
    rt: &Runtime,
    stop: &mut watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut request = stream_url(&rt.cfg).into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        format!("Token {key}")
            .parse()
            .context("Некорректный формат ключа Deepgram")?,
    );
    let (mut socket, _) = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(15), connect_async(request)) =>
            result.context("Таймаут подключения Deepgram")?.context("Не удалось подключиться к Deepgram")?,
        _ = stop.changed() => return Ok(()),
    };
    rt.status("listening", None);
    let mut reconciler = TranscriptReconciler::new();
    let mut last_send = tokio::time::Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut stopping = false;
    let mut shutdown_deadline = tokio::time::Instant::now() + Duration::from_secs(86400);
    let outcome = loop {
        tokio::select! {
            _ = stop.changed(), if !stopping => {
                stopping = true;
                shutdown_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
                // CloseStream asks Deepgram to return remaining final results.
                if let Err(e) = send(&mut socket, Message::Text(r#"{"type":"CloseStream"}"#.into())).await { break Err(e); }
            }
            bytes = audio_rx.recv(), if !stopping => {
                let Some(bytes) = bytes else { break Ok(()); };
                if !rt.paused.load(Ordering::Relaxed) {
                    if let Err(e) = send(&mut socket, Message::Binary(bytes.into())).await { break Err(e); }
                    last_send = tokio::time::Instant::now();
                }
            }
            result = socket.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        let response = match serde_json::from_str::<Response>(&text) {
                            Ok(r) => r, Err(_) => break Err(anyhow!("Некорректный ответ Deepgram")),
                        };
                        let snapshot = match response {
                            Response::Results { is_final, speech_final, from_finalize, channel } => {
                                let text = channel.alternatives.first().map(|a| a.transcript.as_str()).unwrap_or("");
                                reconciler.update(text, is_final, speech_final || (is_final && from_finalize))
                            },
                            Response::UtteranceEnd => None,
                            Response::Error { description } => break Err(anyhow!("Deepgram: {description}")),
                            Response::Other => None,
                        };
                        if let Some(snapshot) = snapshot {
                            if let Err(e) = publish(snapshot, jobs, rt) { break Err(e); }
                            let preview = reconciler.preview();
                            if let Some(ref p) = preview { rt.update(|v| v.source = p.current.clone()); }
                            previews.send_replace(preview);
                        }
                    },
                    Some(Ok(Message::Close(_))) | None => break if stopping { Ok(()) } else { Err(anyhow!("Deepgram закрыл соединение")) },
                    Some(Err(e)) => break Err(anyhow!(e)),
                    Some(Ok(_)) => {},
                }
            }
            _ = tick.tick() => {
                if stopping && tokio::time::Instant::now() >= shutdown_deadline { break Ok(()); }
                if !stopping && last_send.elapsed() >= Duration::from_secs(3) {
                    if let Err(e) = send(&mut socket, Message::Text(r#"{"type":"KeepAlive"}"#.into())).await { break Err(e); }
                    last_send = tokio::time::Instant::now();
                }
                let due = reconciler.flush_due();
                if let Some(snapshot) = due {
                    if let Err(e) = publish(snapshot, jobs, rt) { break Err(e); }
                    let preview = reconciler.preview();
                            if let Some(ref p) = preview { rt.update(|v| v.source = p.current.clone()); }
                            previews.send_replace(preview);
                }
            }
        }
    };
    // This task owns the socket: cancelling it drops the actual connection, with
    // no detached SDK workers or keep-alive tasks surviving session shutdown.
    let _ = tokio::time::timeout(Duration::from_millis(500), socket.close(None)).await;
    if let Some(snapshot) = reconciler.flush() {
        publish(snapshot, jobs, rt)?;
    }
    previews.send_replace(None);
    if let Some(interim) = reconciler.take_interim() {
        let id = rt.add_source(&interim);
        rt.complete(
            id,
            Err("Распознавание прервано: это предварительный текст, требующий проверки.".into()),
        );
    }
    outcome
}

async fn send(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    message: Message,
) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(2), socket.send(message))
        .await
        .context("Таймаут отправки звука")??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finalization_is_bounded_but_never_requested_for_silence_or_shutdown() {
        assert!(!finalize_due(Duration::from_millis(2399), true, false));
        assert!(finalize_due(Duration::from_millis(2400), true, false));
        assert!(!finalize_due(Duration::from_secs(10), false, false));
        assert!(!finalize_due(Duration::from_secs(10), true, true));
    }
    #[test]
    fn websocket_url_matches_pcm_and_contains_no_credentials() {
        let url = stream_url(&crate::config::Config::default());
        assert!(url.contains("encoding=linear16"));
        assert!(url.contains("sample_rate=16000"));
        assert!(url.contains("utterance_end_ms=1000"));
        assert!(!url.contains("Token"));
    }
    #[test]
    fn handles_endpoint_and_metadata_frames() {
        let r: Response = serde_json::from_str(r#"{"type":"Results","channel":{"alternatives":[{"transcript":""}]},"is_final":true,"speech_final":true}"#).unwrap();
        assert!(matches!(
            r,
            Response::Results {
                speech_final: true,
                ..
            }
        ));
        assert!(matches!(
            serde_json::from_str::<Response>(r#"{"type":"Metadata"}"#).unwrap(),
            Response::Other
        ));
        assert!(matches!(
            serde_json::from_str::<Response>(r#"{"type":"UtteranceEnd"}"#).unwrap(),
            Response::UtteranceEnd
        ));
    }
    #[tokio::test]
    #[ignore = "real Deepgram with synthetic WAV, then real OpenAI; never captures system audio"]
    async fn live_coherent_audio_pipeline() {
        let cfg = crate::config::Config {
            source_language: "en".into(),
            target_language: "ru".into(),
            glossary: String::new(),
            ..crate::config::load()
        };
        let key = crate::secure::get_deepgram_key().expect("Deepgram key required");
        let mut request = stream_url(&cfg).into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Authorization", format!("Token {key}").parse().unwrap());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../artifacts");
        let bytes = std::fs::read(root.join("coherent-speech.wav"))
            .expect("Generate 16kHz mono PCM coherent-speech.wav first");
        assert_eq!(&bytes[..4], b"RIFF");
        let mut offset = 12;
        let mut pcm = vec![];
        while offset + 8 <= bytes.len() {
            let n = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            if &bytes[offset..offset + 4] == b"data" {
                pcm = bytes[offset + 8..offset + 8 + n].to_vec();
                break;
            }
            offset += 8 + n + (n % 2);
        }
        assert!(!pcm.is_empty());
        pcm.extend(vec![0; 64000]);
        let (mut ws, _) = connect_async(request).await.unwrap();
        let mut reconciler = TranscriptReconciler::new();
        let mut confirmed = Vec::new();
        let mut packets = Vec::new();
        let mut finals = Vec::new();
        let mut tick = tokio::time::interval(Duration::from_millis(40));
        let mut offset = 0;
        let mut closed = false;
        let start = std::time::Instant::now();
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        if !closed {
                            if offset < pcm.len() {
                                let end = (offset+1280).min(pcm.len());
                                ws.send(Message::Binary(pcm[offset..end].to_vec().into())).await.unwrap();
                                offset = end;
                            } else {
                                ws.send(Message::Text(r#"{"type":"CloseStream"}"#.into())).await.unwrap();
                                closed = true;
                            }
                        }
                        if let Some(s) = reconciler.flush_due() { finals.push(s); }
                    }
                    message = ws.next() => {
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                if let Response::Results { channel, is_final, speech_final, .. } = serde_json::from_str(&text).unwrap() {
                                    let text = channel.alternatives.first().map(|a| a.transcript.as_str()).unwrap_or("");
                                    if is_final && !text.is_empty() { confirmed.push(text.to_string()); }
                                    packets.push(serde_json::json!({"ms":start.elapsed().as_millis(), "text":text, "is_final":is_final, "speech_final":speech_final}));
                                    if let Some(s) = reconciler.update(text, is_final, speech_final) {
                                        if s.is_final { finals.push(s); }
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            Some(Err(e)) => panic!("Deepgram replay failed: {e}"),
                            _ => {}
                        }
                    }
                }
            }
        }).await.expect("Synthetic Deepgram replay timed out");
        if let Some(s) = reconciler.flush() {
            finals.push(s);
        }
        assert_eq!(
            finals
                .iter()
                .map(|s| s.current.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            confirmed.join(" "),
            "No confirmed word may be lost or duplicated"
        );
        assert!(
            reconciler.take_interim().is_none(),
            "CloseStream must finalize the last hypothesis"
        );
        assert!(
            !finals.is_empty() && finals.len() <= 2,
            "Two spoken sentences must not become fragment rows: {:?}",
            finals
        );
        assert!(finals
            .iter()
            .all(|s| s.current.split_whitespace().count() >= 10));
        let client =
            crate::translate::client(crate::secure::get_openai_key().expect("OpenAI key required"));
        let metrics = crate::metrics::Metrics::new();
        let mut result = Vec::new();
        for source in finals {
            let text = tokio::time::timeout(
                Duration::from_secs(20),
                crate::translate::stream_translation(&source, &client, &cfg, &metrics, |_, _| {}),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(text.chars().any(|c| ('а'..='я').contains(&c)));
            result.push(serde_json::json!({"source":source.current, "translation":text}));
        }
        std::fs::write(
            root.join("coherent-audio-replay.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"packets":packets, "sentences":result}))
                .unwrap(),
        )
        .unwrap();
        println!("Synthetic audio pipeline: {} ASR final packets -> {} coherent translation units; {} ms; {}", confirmed.len(), result.len(), start.elapsed().as_millis(), serde_json::to_string(&result).unwrap());
    }
    #[tokio::test]
    #[ignore = "uses saved Deepgram key and a generated synthetic PCM fixture"]
    async fn live_deepgram_synthetic_speech() {
        let key = crate::secure::get_deepgram_key().expect("Deepgram key required");
        let cfg = crate::config::Config {
            source_language: "en".into(),
            ..Default::default()
        };
        let mut request = stream_url(&cfg).into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Authorization", format!("Token {key}").parse().unwrap());
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../artifacts/smoke-speech.wav"),
        )
        .expect("Generate artifacts/smoke-speech.wav using scripts/generate-smoke-audio.ps1");
        assert_eq!(&bytes[..4], b"RIFF");
        let mut offset = 12;
        let mut pcm = vec![];
        while offset + 8 <= bytes.len() {
            let n = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            if &bytes[offset..offset + 4] == b"data" {
                pcm = bytes[offset + 8..offset + 8 + n].to_vec();
                break;
            }
            offset += 8 + n + (n % 2);
        }
        assert!(!pcm.is_empty());
        let second = pcm.clone();
        pcm.extend(vec![0; 16000]);
        pcm.extend(second);
        pcm.extend(vec![0; 32000]);
        let force_finalize = std::env::var_os("DEEPGRAM_TEST_FORCE_FINALIZE").is_some();
        let started = std::time::Instant::now();
        let result=tokio::time::timeout(Duration::from_secs(30),async {
            let (mut ws,_)=connect_async(request).await.unwrap();
            let mut tick=tokio::time::interval(Duration::from_millis(40));
            let mut offset=0; let mut closed=false; let mut transcript=String::new();
            let mut last_finalize=tokio::time::Instant::now();
            let mut finalizes=0; let mut early_final=false;
            loop {
                tokio::select! {
                    _=tick.tick(), if !closed => {
                        if offset<pcm.len() {
                            let end=(offset+1280).min(pcm.len());
                            ws.send(Message::Binary(pcm[offset..end].to_vec().into())).await.unwrap();
                            offset=end;
                            if force_finalize && finalize_due(last_finalize.elapsed(), true, false) {
                                ws.send(Message::Text(r#"{"type":"Finalize"}"#.into())).await.unwrap();
                                last_finalize=tokio::time::Instant::now(); finalizes+=1;
                            }
                        } else {
                            ws.send(Message::Text(r#"{"type":"CloseStream"}"#.into())).await.unwrap(); closed=true;
                        }
                    }
                    response=ws.next()=> {
                        match response {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(Response::Results{channel,is_final:true,..})=serde_json::from_str(&text) {
                                    if let Some(a)=channel.alternatives.first() {
                                        if !a.transcript.is_empty() && !closed { early_final=true; }
                                        transcript.push_str(&a.transcript); transcript.push(' ');
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_)))|None=>break,
                            Some(Err(e))=>panic!("Deepgram smoke failed: {e}"),
                            _=>{}
                        }
                    }
                }
            }
            if force_finalize { assert!(finalizes > 0 && early_final, "Finalize must work without closing the stream"); }
            transcript
        }).await.expect("Deepgram smoke timeout");
        assert!(
            result.to_ascii_lowercase().matches("experience").count() >= 2,
            "Unexpected synthetic transcript: {result}"
        );
        println!(
            "Deepgram synthetic speech: {} ms; transcript: {}",
            started.elapsed().as_millis(),
            result.trim()
        );
    }
}
