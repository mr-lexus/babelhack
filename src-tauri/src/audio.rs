use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tokio::sync::mpsc;

const DST_RATE: f64 = 16000.0;
const CHUNK_SAMPLES: usize = 640;

pub struct AudioCapture {
    // Fields drop in declaration order: release callbacks before destroying the CoreAudio tap.
    _stream: cpal::Stream,
    #[cfg(target_os = "macos")]
    _tap: super::macos_tap::Tap,
}

impl AudioCapture {
    /// Captures system output through the platform loopback backend and pushes 16 kHz mono
    /// i16 PCM chunks (1280 bytes / 40 ms each) into `tx`.
    ///
    /// `device_name` pins the output by stable ID (legacy names remain accepted);
    /// when absent, the default output device is used. A missing selected device is an error.
    ///
    /// Returns the capture handle and a receiver for stream errors.
    /// When the receiver gets a message, the stream has failed and should be restarted.
    pub fn start(
        tx: mpsc::Sender<Vec<u8>>,
        device_name: Option<&str>,
        level: Arc<AtomicU32>,
        dropped: Arc<AtomicU32>,
    ) -> anyhow::Result<(Self, tokio::sync::mpsc::UnboundedReceiver<String>)> {
        let host = capture_host()?;
        let output = resolve_output_device(&host, device_name)?;
        let device_name = output.description()?.name().to_string();
        #[cfg(target_os = "linux")]
        let device = linux_monitor(&host, &output)?;
        #[cfg(target_os = "macos")]
        let (device, tap) = super::macos_tap::create(&host, &output)
            .context("РќРµ СѓРґР°Р»РѕСЃСЊ Р·Р°С…РІР°С‚РёС‚СЊ СЃРёСЃС‚РµРјРЅС‹Р№ Р·РІСѓРє macOS. Р Р°Р·СЂРµС€РёС‚Рµ Р·Р°РїРёСЃСЊ СЃРёСЃС‚РµРјРЅРѕРіРѕ Р°СѓРґРёРѕ РІ РЅР°СЃС‚СЂРѕР№РєР°С… РєРѕРЅС„РёРґРµРЅС†РёР°Р»СЊРЅРѕСЃС‚Рё Рё РїРµСЂРµР·Р°РїСѓСЃС‚РёС‚Рµ РїСЂРёР»РѕР¶РµРЅРёРµ")?;
        #[cfg(target_os = "windows")]
        let device = output;
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let config = device.default_input_config().context(
            "Р¤РѕСЂРјР°С‚ СЃРёСЃС‚РµРјРЅРѕРіРѕ Р°СѓРґРёРѕР·Р°С…РІР°С‚Р° РЅРµРґРѕСЃС‚СѓРїРµРЅ",
        )?;
        #[cfg(target_os = "windows")]
        let config = device.default_output_config().context(
            "Р¤РѕСЂРјР°С‚ СЃРёСЃС‚РµРјРЅРѕРіРѕ Р°СѓРґРёРѕР·Р°С…РІР°С‚Р° РЅРµРґРѕСЃС‚СѓРїРµРЅ",
        )?;
        let sample_rate = config.sample_rate() as f64;
        let channels = config.channels() as usize;
        let sample_format = config.sample_format();
        log::info!(
            "loopback device '{}': {} Hz, {} ch, {:?}",
            device_name,
            sample_rate as u32,
            channels,
            sample_format
        );

        let mut conv = Converter::new(sample_rate, channels);
        conv.level = level;
        conv.dropped = dropped;

        let (error_tx, error_rx) = tokio::sync::mpsc::unbounded_channel();
        let err_tx = error_tx.clone();

        macro_rules! capture {
            ($sample:ty) => {
                device.build_input_stream(
                    config.into(),
                    move |data: &[$sample], _| conv.push(data, &tx),
                    move |err| {
                        let _ = err_tx.send(err.to_string());
                    },
                    None,
                )?
            };
        }
        let stream = match sample_format {
            cpal::SampleFormat::F32 => capture!(f32),
            cpal::SampleFormat::F64 => capture!(f64),
            cpal::SampleFormat::I8 => capture!(i8),
            cpal::SampleFormat::I16 => capture!(i16),
            cpal::SampleFormat::I24 => capture!(cpal::I24),
            cpal::SampleFormat::I32 => capture!(i32),
            cpal::SampleFormat::I64 => capture!(i64),
            cpal::SampleFormat::U8 => capture!(u8),
            cpal::SampleFormat::U16 => capture!(u16),
            cpal::SampleFormat::U24 => capture!(cpal::U24),
            cpal::SampleFormat::U32 => capture!(u32),
            cpal::SampleFormat::U64 => capture!(u64),
            other => {
                return Err(anyhow!(
                    "РќРµРїРѕРґРґРµСЂР¶РёРІР°РµРјС‹Р№ С„РѕСЂРјР°С‚ Р°СѓРґРёРѕ: {other:?}"
                ))
            }
        };

        stream
            .play()
            .context("failed to start loopback capture stream")?;
        log::info!("loopback capture started");
        Ok((
            AudioCapture {
                _stream: stream,
                #[cfg(target_os = "macos")]
                _tap: tap,
            },
            error_rx,
        ))
    }
}

#[derive(Clone, serde::Serialize)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
}

fn capture_host() -> anyhow::Result<cpal::Host> {
    #[cfg(target_os = "linux")]
    let id = cpal::HostId::PulseAudio;
    #[cfg(target_os = "macos")]
    let id = cpal::HostId::CoreAudio;
    #[cfg(target_os = "windows")]
    let id = cpal::HostId::Wasapi;
    cpal::host_from_id(id).context("РђСѓРґРёРѕСЃРµСЂРІРёСЃ РЅРµРґРѕСЃС‚СѓРїРµРЅ. РќР° Linux Р·Р°РїСѓСЃС‚РёС‚Рµ PulseAudio РёР»Рё PipeWire СЃ pipewire-pulse РІ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєРѕР№ СЃРµСЃСЃРёРё")
}

pub fn list_output_devices() -> anyhow::Result<Vec<OutputDevice>> {
    let host = capture_host()?;
    host.output_devices()?
        .map(|d| {
            Ok(OutputDevice {
                id: d.id()?.to_string(),
                name: d.description()?.name().to_string(),
            })
        })
        .collect()
}

fn selected_index(devices: &[OutputDevice], saved: &str) -> anyhow::Result<usize> {
    if let Some(i) = devices.iter().position(|d| d.id == saved) {
        return Ok(i);
    }
    // Migrate legacy display-name selections only when the match is unambiguous.
    let matches: Vec<_> = devices
        .iter()
        .enumerate()
        .filter(|(_, d)| d.name == saved)
        .collect();
    match matches.as_slice() {
        [(i, _)] => Ok(*i),
        [] => Err(anyhow!(
            "Р’С‹Р±СЂР°РЅРЅРѕРµ Р°СѓРґРёРѕСѓСЃС‚СЂРѕР№СЃС‚РІРѕ РЅРµРґРѕСЃС‚СѓРїРЅРѕ. Р’С‹Р±РµСЂРёС‚Рµ РґСЂСѓРіРѕРµ РІ РЅР°СЃС‚СЂРѕР№РєР°С…."
        )),
        _ => Err(anyhow!(
            "РќРµСЃРєРѕР»СЊРєРѕ СѓСЃС‚СЂРѕР№СЃС‚РІ РёРјРµСЋС‚ РѕРґРёРЅР°РєРѕРІРѕРµ РёРјСЏ. Р’С‹Р±РµСЂРёС‚Рµ СѓСЃС‚СЂРѕР№СЃС‚РІРѕ Р·Р°РЅРѕРІРѕ."
        )),
    }
}

fn resolve_output_device(host: &cpal::Host, saved: Option<&str>) -> anyhow::Result<cpal::Device> {
    let Some(saved) = saved else {
        return host
            .default_output_device()
            .ok_or_else(|| anyhow!("РЈСЃС‚СЂРѕР№СЃС‚РІРѕ РІРѕСЃРїСЂРѕРёР·РІРµРґРµРЅРёСЏ РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ РЅРµРґРѕСЃС‚СѓРїРЅРѕ"));
    };
    let devices: Vec<_> = host.output_devices()?.collect();
    let descriptors: anyhow::Result<Vec<_>> = devices
        .iter()
        .map(|d| {
            Ok(OutputDevice {
                id: d.id()?.to_string(),
                name: d.description()?.name().to_string(),
            })
        })
        .collect();
    let index = selected_index(&descriptors?, saved)?;
    Ok(devices
        .into_iter()
        .nth(index)
        .expect("validated device index"))
}

#[cfg(target_os = "linux")]
fn linux_monitor(host: &cpal::Host, output: &cpal::Device) -> anyhow::Result<cpal::Device> {
    // Query the server through its public API; CPAL does not expose PulseAudio's
    // Sink/Source types. Never guess a ".monitor" suffix or fall back to a microphone.
    let output_id = output.id()?;
    let sink_name = std::ffi::CString::new(output_id.id())?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<String> {
            let client = pulseaudio::Client::from_env(c"babelhack-monitor-lookup")?;
            futures::executor::block_on(async {
                let sink = client.sink_info_by_name(sink_name).await?;
                let monitor = sink
                    .monitor_source_index
                    .ok_or_else(|| anyhow!("У выбранного выхода нет monitor-источника"))?;
                let source = client.source_info(monitor).await?;
                if source.monitor_of_sink_index != Some(sink.index) {
                    return Err(anyhow!("Monitor не принадлежит выбранному выходу"));
                }
                Ok(source.name.to_str()?.to_owned())
            })
        })();
        let _ = tx.send(result);
    });
    let name = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .context("PulseAudio: превышено время поиска monitor-источника")??;
    let monitor_id = cpal::DeviceId::new(cpal::HostId::PulseAudio, name);
    host.input_devices()?
        .find(|device| device.id().is_ok_and(|id| id == monitor_id))
        .ok_or_else(|| {
            anyhow!("Monitor выбранного выхода недоступен. Проверьте PulseAudio / pipewire-pulse.")
        })
}

/// Downmixes N-channel interleaved samples to mono, linearly resamples to 16 kHz,
/// converts to i16, and emits fixed-size chunks.
struct Converter {
    src_rate: f64,
    channels: usize,
    prev: f32,
    idx: f64,
    out: Vec<i16>,
    level: Arc<AtomicU32>,
    dropped: Arc<AtomicU32>,
}

impl Converter {
    fn new(src_rate: f64, channels: usize) -> Self {
        Self {
            src_rate,
            channels: channels.max(1),
            prev: 0.0,
            idx: 0.0,
            out: Vec::with_capacity(CHUNK_SAMPLES * 2),
            level: Arc::new(AtomicU32::new(0)),
            dropped: Arc::new(AtomicU32::new(0)),
        }
    }

    fn push<T>(&mut self, data: &[T], tx: &mpsc::Sender<Vec<u8>>)
    where
        T: cpal::Sample,
    {
        if data.is_empty() {
            return;
        }
        let step = self.src_rate / DST_RATE;
        let frames = data.len() / self.channels;
        let mut mono = Vec::with_capacity(frames);
        for f in 0..frames {
            let mut acc = 0.0f32;
            for c in 0..self.channels {
                acc += data[f * self.channels + c]
                    .to_float_sample()
                    .to_sample::<f32>();
            }
            mono.push(acc / self.channels as f32);
        }
        if mono.is_empty() {
            return;
        }
        let rms = (mono.iter().map(|v| v * v).sum::<f32>() / mono.len() as f32).sqrt();
        self.level
            .store(rms.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        let n = mono.len() as isize;
        let mut idx = self.idx;
        loop {
            let i = idx.floor() as isize;
            if i >= n {
                break;
            }
            let i1 = i + 1;
            if i1 >= n {
                break;
            }
            let frac = (idx - i as f64) as f32;
            let x0 = if i >= 0 { mono[i as usize] } else { self.prev };
            let x1 = mono[i1 as usize];
            let v = x0 + (x1 - x0) * frac;
            let sample = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
            self.out.push(sample);
            idx += step;
        }
        self.idx = idx - n as f64;
        if let Some(&last) = mono.last() {
            self.prev = last;
        }
        self.flush(tx);
    }

    fn flush(&mut self, tx: &mpsc::Sender<Vec<u8>>) {
        while self.out.len() >= CHUNK_SAMPLES {
            let chunk: Vec<u8> = self
                .out
                .drain(..CHUNK_SAMPLES)
                .flat_map(|v| v.to_le_bytes())
                .collect();
            match tx.try_send(chunk) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_uses_stable_id_and_never_silently_switches_devices() {
        let devices = vec![
            OutputDevice {
                id: "id:1".into(),
                name: "Headset".into(),
            },
            OutputDevice {
                id: "id:2".into(),
                name: "Headset".into(),
            },
            OutputDevice {
                id: "id:3".into(),
                name: "Speakers".into(),
            },
        ];
        assert_eq!(selected_index(&devices, "id:2").unwrap(), 1);
        assert_eq!(selected_index(&devices, "Speakers").unwrap(), 2);
        assert!(selected_index(&devices, "Headset").is_err());
        assert!(selected_index(&devices, "missing").is_err());
    }
    #[test]
    #[ignore = "requires an explicitly selected synthetic test output; never run against private audio"]
    fn platform_loopback_smoke() {
        let device = std::env::var("INTERVIEW_TRANSLATOR_TEST_AUDIO_DEVICE")
            .expect("Set an isolated test output ID");
        let (tx, mut rx) = mpsc::channel(50);
        let level = Arc::new(AtomicU32::new(0));
        let (capture, _errors) = AudioCapture::start(
            tx,
            Some(&device),
            level.clone(),
            Arc::new(AtomicU32::new(0)),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let mut audible = false;
        while std::time::Instant::now() < deadline {
            while let Ok(bytes) = rx.try_recv() {
                assert_eq!(bytes.len(), CHUNK_SAMPLES * 2);
                audible |= bytes
                    .chunks_exact(2)
                    .any(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs() > 100);
            }
            if audible {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        drop(capture);
        assert!(audible, "No audible loopback PCM from isolated test output");
    }
    #[test]
    fn resampling_is_independent_of_callback_boundaries() {
        for rate in [16000.0, 44100.0, 48000.0] {
            let samples: Vec<f32> = (0..9600)
                .map(|i| ((i as f32) * 0.012).sin() * 0.5)
                .collect();
            let (a, mut ar) = mpsc::channel(100);
            let (b, mut br) = mpsc::channel(100);
            Converter::new(rate, 1).push(&samples, &a);
            let mut split = Converter::new(rate, 1);
            for chunk in samples.chunks(127) {
                split.push(chunk, &b);
                split.push::<f32>(&[], &b);
            }
            let mut av = vec![];
            while let Ok(c) = ar.try_recv() {
                av.extend(c);
            }
            let mut bv = vec![];
            while let Ok(c) = br.try_recv() {
                bv.extend(c);
            }
            assert_eq!(av.len(), bv.len());
            // Floating point rounding may differ by one PCM unit.
            for (a, b) in av.chunks_exact(2).zip(bv.chunks_exact(2)) {
                let x = i16::from_le_bytes([a[0], a[1]]) as i32;
                let y = i16::from_le_bytes([b[0], b[1]]) as i32;
                assert!((x - y).abs() <= 1);
            }
        }
    }
    #[test]
    fn audio_backpressure_is_bounded_and_measured() {
        let (tx, _rx) = mpsc::channel(1);
        let mut c = Converter::new(16000.0, 2);
        c.push(&vec![0.25f32; 640 * 2 * 5], &tx);
        assert!(c.dropped.load(Ordering::Relaxed) >= 3);
        assert!(c.out.len() < CHUNK_SAMPLES);
    }
}
