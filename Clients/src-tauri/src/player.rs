//! Audio playback in Rust: decode the whole clip (Ogg Opus through libopus, AAC/MP3/WAV/... through symphonia), play it
//! through cpal and keep the UI informed. Speed 1x / 1.5x / 2x uses the WSOLA stretcher, so the pitch stays natural.
//!
//! The cpal stream is owned by a dedicated thread (streams are not `Send` on every platform). With
//! `WORKCHAT_AUDIO_NULL=1` (or when no output device exists) a silent real-time clock is used instead, which keeps
//! the UI and the tests working on machines without sound.

use crate::audio::{self, Stretcher};
use crate::engine::Emit;
use crate::view::CoreEvent;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_SECONDS: u32 = 40 * 60;

pub enum Cmd {
    Play { id: String, path: PathBuf, speed: f32, from_ms: u32 },
    Pause,
    Resume,
    Seek(u32),
    Speed(f32),
    Stop,
}

#[derive(Clone)]
pub struct Player {
    tx: Sender<Cmd>,
}

impl Player {
    pub fn new(emit: Emit) -> Player {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("audio-player".into()).spawn(move || run(rx, emit)).expect("audio thread");
        Player { tx }
    }
    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

type Slot = Arc<Mutex<Option<Stretcher>>>;

struct Out {
    _stream: Option<cpal::Stream>,
    rate: u32,
    playing: Arc<AtomicBool>,
    slot: Slot,
}

fn build<T>(device: &cpal::Device, cfg: &cpal::StreamConfig, slot: Slot, playing: Arc<AtomicBool>) -> std::result::Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    let ch = cfg.channels as usize;
    let mut tmp: Vec<f32> = Vec::new();
    device.build_output_stream(
        cfg,
        move |data: &mut [T], _| {
            let frames = data.len() / ch.max(1);
            tmp.resize(frames, 0.0);
            let mut ok = false;
            if playing.load(Ordering::Relaxed) {
                if let Ok(mut g) = slot.try_lock() {
                    if let Some(st) = g.as_mut() {
                        st.read(&mut tmp);
                        ok = true;
                    }
                }
            }
            if !ok {
                tmp.iter_mut().for_each(|x| *x = 0.0);
            }
            for (i, f) in data.chunks_mut(ch.max(1)).enumerate() {
                let v = T::from_sample(tmp[i]);
                for s in f.iter_mut() {
                    *s = v;
                }
            }
        },
        |e| tracing::warn!("audio output error: {e}"),
        None,
    )
}

fn open_out() -> Option<Out> {
    let playing = Arc::new(AtomicBool::new(false));
    let slot: Slot = Arc::new(Mutex::new(None));
    if std::env::var("WORKCHAT_AUDIO_NULL").is_ok() {
        return Some(Out { _stream: None, rate: 48_000, playing, slot });
    }
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let supported = device.default_output_config().ok()?;
    let rate = supported.sample_rate().0;
    let fmt = supported.sample_format();
    let cfg: cpal::StreamConfig = supported.into();
    let stream = match fmt {
        cpal::SampleFormat::F32 => build::<f32>(&device, &cfg, slot.clone(), playing.clone()),
        cpal::SampleFormat::I16 => build::<i16>(&device, &cfg, slot.clone(), playing.clone()),
        cpal::SampleFormat::U16 => build::<u16>(&device, &cfg, slot.clone(), playing.clone()),
        cpal::SampleFormat::I32 => build::<i32>(&device, &cfg, slot.clone(), playing.clone()),
        _ => return None,
    }
    .ok()?;
    stream.play().ok()?;
    Some(Out { _stream: Some(stream), rate, playing, slot })
}

fn run(rx: mpsc::Receiver<Cmd>, emit: Emit) {
    let mut out: Option<Out> = None;
    let mut out_failed = false;
    let mut current: Option<(String, u32 /*duration ms*/)> = None;
    let mut speed = 1.0f32;
    let mut null_clock: Option<(Instant, f64)> = None; // silent mode: (started, start position in samples)
    let mut last_emit = Instant::now();
    let mut state = "idle";
    let state_event = |id: &str, st: &str, pos: u32, dur: u32, speed: f32, err: Option<String>| {
        emit(CoreEvent::Audio { id: id.to_string(), state: st.to_string(), pos_ms: pos, dur_ms: dur, speed, error: err });
    };
    loop {
        let cmd = match rx.recv_timeout(Duration::from_millis(80)) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let Some(c) = cmd {
            match c {
                Cmd::Play { id, path, speed: sp, from_ms } => {
                    if out.is_none() && !out_failed {
                        out = open_out();
                        out_failed = out.is_none();
                        if out_failed {
                            tracing::warn!("no audio output device; playing silently");
                            out = Some(Out { _stream: None, rate: 48_000, playing: Arc::new(AtomicBool::new(false)), slot: Arc::new(Mutex::new(None)) });
                        }
                    }
                    let o = out.as_mut().unwrap();
                    state_event(&id, "loading", 0, 0, sp, None);
                    o.playing.store(false, Ordering::SeqCst);
                    match audio::decode_file(&path, MAX_SECONDS) {
                        Ok(pcm) => {
                            let samples = audio::resample(&pcm.samples, pcm.rate, o.rate);
                            let dur = (samples.len() as u64 * 1000 / o.rate as u64) as u32;
                            let mut st = Stretcher::new(Arc::new(samples), o.rate);
                            st.speed = sp as f64;
                            st.seek((from_ms as u64 * o.rate as u64 / 1000) as usize);
                            null_clock = Some((Instant::now(), st.pos));
                            *o.slot.lock().unwrap() = Some(st);
                            o.playing.store(true, Ordering::SeqCst);
                            speed = sp;
                            current = Some((id.clone(), dur));
                            state = "playing";
                            state_event(&id, "playing", from_ms, dur, sp, None);
                        }
                        Err(e) => {
                            current = None;
                            state = "idle";
                            state_event(&id, "error", 0, 0, sp, Some(e.to_string()));
                        }
                    }
                }
                Cmd::Pause => {
                    if let (Some(o), Some((id, dur))) = (out.as_ref(), current.as_ref()) {
                        o.playing.store(false, Ordering::SeqCst);
                        state = "paused";
                        let pos = pos_ms(o, null_clock, speed);
                        state_event(id, "paused", pos, *dur, speed, None);
                    }
                }
                Cmd::Resume => {
                    if let (Some(o), Some((id, dur))) = (out.as_ref(), current.as_ref()) {
                        let restart = o.slot.lock().unwrap().as_ref().map(|s| s.finished()).unwrap_or(false);
                        if restart {
                            if let Some(s) = o.slot.lock().unwrap().as_mut() {
                                s.seek(0);
                            }
                        }
                        let p = o.slot.lock().unwrap().as_ref().map(|s| s.pos).unwrap_or(0.0);
                        null_clock = Some((Instant::now(), p));
                        o.playing.store(true, Ordering::SeqCst);
                        state = "playing";
                        state_event(id, "playing", pos_ms(o, null_clock, speed), *dur, speed, None);
                    }
                }
                Cmd::Seek(ms) => {
                    if let (Some(o), Some((id, dur))) = (out.as_ref(), current.as_ref()) {
                        let mut pos = 0.0;
                        if let Some(s) = o.slot.lock().unwrap().as_mut() {
                            s.seek((ms as u64 * o.rate as u64 / 1000) as usize);
                            pos = s.pos;
                        }
                        null_clock = Some((Instant::now(), pos));
                        state_event(id, state, ms, *dur, speed, None);
                    }
                }
                Cmd::Speed(sp) => {
                    speed = sp;
                    if let (Some(o), Some((id, dur))) = (out.as_ref(), current.as_ref()) {
                        let mut pos = 0.0;
                        if let Some(s) = o.slot.lock().unwrap().as_mut() {
                            s.speed = sp as f64;
                            pos = s.pos;
                        }
                        null_clock = Some((Instant::now(), pos));
                        state_event(id, state, pos_ms(o, null_clock, speed), *dur, speed, None);
                    }
                }
                Cmd::Stop => {
                    if let Some(o) = out.as_ref() {
                        o.playing.store(false, Ordering::SeqCst);
                        *o.slot.lock().unwrap() = None;
                    }
                    if let Some((id, dur)) = current.take() {
                        state_event(&id, "stopped", 0, dur, speed, None);
                    }
                    state = "idle";
                }
            }
        }
        // Periodic position report; also ends playback.
        if state == "playing" && last_emit.elapsed() >= Duration::from_millis(80) {
            last_emit = Instant::now();
            if let (Some(o), Some((id, dur))) = (out.as_ref(), current.as_ref()) {
                if o._stream.is_none() {
                    // silent clock: advance the stretcher position by wall time
                    if let (Some((t0, p0)), Some(s)) = (null_clock, o.slot.lock().unwrap().as_mut()) {
                        s.pos = (p0 + t0.elapsed().as_secs_f64() * o.rate as f64 * speed as f64).min(s.len() as f64);
                        s.seek(s.pos as usize);
                        s.pos = s.pos.max(0.0);
                    }
                }
                let finished = o.slot.lock().unwrap().as_ref().map(|s| s.finished()).unwrap_or(true) || (o._stream.is_none() && pos_ms(o, null_clock, speed) >= *dur);
                let pos = if finished { *dur } else { pos_ms(o, null_clock, speed) };
                if finished {
                    o.playing.store(false, Ordering::SeqCst);
                    state = "ended";
                    state_event(id, "ended", pos, *dur, speed, None);
                } else {
                    state_event(id, "playing", pos, *dur, speed, None);
                }
            }
        }
    }
}

fn pos_ms(o: &Out, _clock: Option<(Instant, f64)>, _speed: f32) -> u32 {
    o.slot.lock().unwrap().as_ref().map(|s| (s.position() as u64 * 1000 / o.rate as u64) as u32).unwrap_or(0)
}
