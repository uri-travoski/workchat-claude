//! Voice message recording: capture from the microphone with cpal (CoreAudio, WASAPI, ALSA, Oboe on Android), resample
//! to 48 kHz, encode Ogg Opus on the fly and report levels for the live waveform.
//!
//! For tests and screenshots without a microphone, `WORKCHAT_FAKE_MIC=/path/to/audio` feeds that file in real time.

use crate::audio::{self, OggOpusWriter, OPUS_RATE};
use crate::engine::Emit;
use crate::error::{CoreError, Result};
use crate::view::CoreEvent;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct VoiceResult {
    pub path: PathBuf,
    pub duration_ms: u32,
    /// Up to 64 values 0..=255.
    pub peaks: Vec<u8>,
}

pub struct VoiceSession {
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<Result<VoiceResult>>,
}

impl VoiceSession {
    /// Stops recording and returns the finished file.
    pub fn finish(self) -> Result<VoiceResult> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.join().map_err(|_| CoreError::local("The recorder stopped unexpectedly."))?
    }
    pub fn cancel(self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.handle.join();
    }
}

/// Resamples a stream of chunks (linear interpolation with carried phase) to 48 kHz.
pub struct StreamResampler {
    ratio: f64, // input samples per output sample
    pos: f64,   // position of the next output sample in the input, relative to the current chunk start (can be -1 .. )
    last: f32,
}

impl StreamResampler {
    pub fn new(from: u32) -> Self {
        StreamResampler { ratio: from as f64 / OPUS_RATE as f64, pos: 0.0, last: 0.0 }
    }
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if (self.ratio - 1.0).abs() < 1e-9 {
            out.extend_from_slice(input);
            return;
        }
        // index -1 is the last sample of the previous chunk
        let get = |i: isize, last: f32| if i < 0 { last } else { input[i as usize] };
        while self.pos + 1.0 < input.len() as f64 {
            let k = self.pos.floor() as isize;
            let frac = (self.pos - k as f64) as f32;
            let a = get(k, self.last);
            let b = get(k + 1, self.last);
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        self.pos -= input.len() as f64;
        if let Some(l) = input.last() {
            self.last = *l;
        }
    }
}

enum Source {
    Fake(audio::Pcm),
    Device,
}

fn fake_source() -> Option<Source> {
    let p = std::env::var("WORKCHAT_FAKE_MIC").ok()?;
    audio::decode_file(Path::new(&p), 600).ok().map(Source::Fake)
}

fn build_input<T>(device: &cpal::Device, cfg: &cpal::StreamConfig, tx: mpsc::Sender<Vec<f32>>) -> std::result::Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let ch = cfg.channels as usize;
    device.build_input_stream(
        cfg,
        move |data: &[T], _| {
            let mono: Vec<f32> = data.chunks(ch.max(1)).map(|f| f.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / ch.max(1) as f32).collect();
            let _ = tx.send(mono);
        },
        |e| tracing::warn!("microphone stream error: {e}"),
        None,
    )
}

fn open_device(tx: mpsc::Sender<Vec<f32>>) -> Result<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| CoreError::local("No microphone was found."))?;
    let supported = device.default_input_config().map_err(|e| CoreError::local(format!("Cannot use the microphone: {e}")))?;
    let rate = supported.sample_rate().0;
    let fmt = supported.sample_format();
    let cfg: cpal::StreamConfig = supported.into();
    let stream = match fmt {
        cpal::SampleFormat::F32 => build_input::<f32>(&device, &cfg, tx),
        cpal::SampleFormat::I16 => build_input::<i16>(&device, &cfg, tx),
        cpal::SampleFormat::U16 => build_input::<u16>(&device, &cfg, tx),
        cpal::SampleFormat::I32 => build_input::<i32>(&device, &cfg, tx),
        other => return Err(CoreError::local(format!("Unsupported microphone format {other:?}."))),
    }
    .map_err(|e| CoreError::local(format!("Cannot open the microphone: {e}")))?;
    stream.play().map_err(|e| CoreError::local(format!("Cannot start the microphone: {e}")))?;
    Ok((stream, rate))
}

/// Starts recording into `<dir>/voice-<date>-<time>.ogg`. Errors (no microphone, permission) are returned here.
pub fn start(dir: &Path, name: &str, emit: Emit) -> Result<VoiceSession> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(name);
    let file = std::fs::File::create(&path)?;
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
    let (s2, c2, p2) = (stop.clone(), cancel.clone(), path.clone());
    let handle = std::thread::Builder::new()
        .name("voice-record".into())
        .spawn(move || run(file, p2, s2, c2, emit, ready_tx))
        .map_err(|e| CoreError::local(e.to_string()))?;
    match ready_rx.recv_timeout(Duration::from_secs(8)) {
        Ok(Ok(())) => Ok(VoiceSession { stop, cancel, handle }),
        Ok(Err(e)) => {
            let _ = handle.join();
            let _ = std::fs::remove_file(&path);
            Err(e)
        }
        Err(_) => Err(CoreError::local("The microphone did not start.")),
    }
}

fn run(file: std::fs::File, path: PathBuf, stop: Arc<AtomicBool>, cancel: Arc<AtomicBool>, emit: Emit, ready: mpsc::Sender<Result<()>>) -> Result<VoiceResult> {
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let mut _stream = None;
    let mut feeder: Option<std::thread::JoinHandle<()>> = None;
    let rate;
    match fake_source().unwrap_or(Source::Device) {
        Source::Fake(pcm) => {
            rate = pcm.rate;
            let (stop2, tx2) = (stop.clone(), tx.clone());
            feeder = Some(std::thread::spawn(move || {
                let chunk = (pcm.rate / 100) as usize;
                let mut i = 0;
                while !stop2.load(Ordering::SeqCst) {
                    let end = (i + chunk).min(pcm.samples.len());
                    if i >= pcm.samples.len() {
                        i = 0;
                        continue;
                    }
                    let _ = tx2.send(pcm.samples[i..end].to_vec());
                    i = end;
                    std::thread::sleep(Duration::from_millis(10));
                }
            }));
        }
        Source::Device => match open_device(tx.clone()) {
            Ok((s, r)) => {
                rate = r;
                _stream = Some(s);
            }
            Err(e) => {
                let _ = ready.send(Err(e));
                return Err(CoreError::local("no microphone"));
            }
        },
    }
    drop(tx);
    let _ = ready.send(Ok(()));
    let mut writer = OggOpusWriter::new(file, 24_000)?;
    let mut rs = StreamResampler::new(rate);
    let mut levels: Vec<f32> = Vec::new();
    let mut win: Vec<f32> = Vec::new();
    let win_len = (OPUS_RATE / 20) as usize; // 50 ms
    let mut last_emit = Instant::now();
    let mut smooth = 0.0f32;
    let mut buf = Vec::new();
    loop {
        let stopping = stop.load(Ordering::SeqCst);
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(chunk) => {
                buf.clear();
                rs.process(&chunk, &mut buf);
                writer.push(&buf)?;
                win.extend_from_slice(&buf);
                while win.len() >= win_len {
                    let rms = (win[..win_len].iter().map(|x| x * x).sum::<f32>() / win_len as f32).sqrt();
                    levels.push(rms);
                    smooth = smooth * 0.5 + rms * 0.5;
                    win.drain(..win_len);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stopping {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if last_emit.elapsed() >= Duration::from_millis(60) {
            last_emit = Instant::now();
            // 0..1, roughly -50 dB .. 0 dB
            let db = 20.0 * smooth.max(1e-5).log10();
            let level = ((db + 50.0) / 50.0).clamp(0.0, 1.0);
            emit(CoreEvent::Voice { level, elapsed_ms: (writer.total * 1000 / OPUS_RATE as u64) as u32 });
        }
        if stopping && rx.try_recv().is_err() {
            break;
        }
    }
    drop(_stream);
    if let Some(f) = feeder {
        let _ = f.join();
    }
    let total = writer.total;
    writer.finish()?;
    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(&path);
        return Err(CoreError::local("Recording cancelled."));
    }
    let duration_ms = (total * 1000 / OPUS_RATE as u64) as u32;
    let peaks = audio::levels_to_peaks(&audio::reduce_levels(&levels, 64));
    Ok(VoiceResult { path, duration_ms, peaks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_resampler_keeps_length_over_chunks() {
        let mut rs = StreamResampler::new(44_100);
        let mut out = Vec::new();
        let src: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.05).sin()).collect();
        for c in src.chunks(441) {
            rs.process(c, &mut out);
        }
        assert!((out.len() as i64 - 48_000).abs() < 8, "{}", out.len());
        let mut same = StreamResampler::new(48_000);
        let mut o = Vec::new();
        same.process(&[0.1, 0.2], &mut o);
        assert_eq!(o, vec![0.1, 0.2]);
    }

    #[test]
    fn records_from_a_fake_microphone_to_ogg_opus() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.wav");
        std::env::set_var("WORKCHAT_FAKE_MIC", &fixture);
        let levels = Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
        let l2 = levels.clone();
        let emit: Emit = Arc::new(move |e| {
            if let CoreEvent::Voice { level, .. } = e {
                l2.lock().unwrap().push(level);
            }
        });
        let s = start(dir.path(), "voice-test.ogg", emit).unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        let r = s.finish().unwrap();
        std::env::remove_var("WORKCHAT_FAKE_MIC");
        assert!((900..=1700).contains(&r.duration_ms), "{}", r.duration_ms);
        assert!(!r.peaks.is_empty() && r.peaks.len() <= 64);
        assert!(r.peaks.iter().any(|p| *p > 200));
        assert!(levels.lock().unwrap().iter().any(|l| *l > 0.3));
        let data = std::fs::read(&r.path).unwrap();
        assert!(audio::is_ogg_opus(&data));
        let back = audio::decode_ogg_opus(&data).unwrap();
        assert!((back.duration_ms() as i64 - r.duration_ms as i64).abs() < 60);
    }
}
