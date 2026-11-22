//! Audio codecs in Rust, so voice messages behave the same on every platform and never depend on what a webview can
//! decode (WebKitGTK and iOS WKWebView cannot play Ogg Opus, and `getUserMedia` is unreliable in them).
//!
//! * Encoding: Ogg Opus (libopus built from source by `audiopus_sys`, `ogg` crate for the container).
//! * Decoding: Ogg Opus through libopus, everything else (AAC/M4A, MP3, Vorbis, FLAC, WAV, ALAC) through symphonia.
//! * Everything works on mono `f32` PCM. Peaks for the waveform come from [`peaks`].

use crate::error::{CoreError, Result};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::io::Write;
use std::path::Path;

pub const OPUS_RATE: u32 = 48_000;
const FRAME: usize = 960; // 20 ms at 48 kHz

/// Decoded audio: mono, `rate` Hz.
#[derive(Debug, Clone)]
pub struct Pcm {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Pcm {
    pub fn duration_ms(&self) -> u32 {
        if self.rate == 0 {
            return 0;
        }
        (self.samples.len() as u64 * 1000 / self.rate as u64) as u32
    }
}

fn audio_err(e: impl std::fmt::Display) -> CoreError {
    CoreError::local(format!("Audio error: {e}"))
}

// ---------------------------------------------------------------- resampling and peaks

/// Linear-interpolation resampler. Good enough for speech and for playback at the device rate.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let pos = i as f64 * ratio;
        let k = pos as usize;
        let frac = (pos - k as f64) as f32;
        let a = input[k];
        let b = *input.get(k + 1).unwrap_or(&a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// `n` loudness values 0..=255 (RMS per slice, scaled so the loudest slice is 255), as the server's waveform meta wants.
pub fn peaks(samples: &[f32], n: usize) -> Vec<u8> {
    if samples.is_empty() || n == 0 {
        return vec![];
    }
    let n = n.min(samples.len());
    let mut rms = Vec::with_capacity(n);
    for i in 0..n {
        let a = i * samples.len() / n;
        let b = ((i + 1) * samples.len() / n).max(a + 1).min(samples.len());
        let s: f32 = samples[a..b].iter().map(|x| x * x).sum::<f32>() / (b - a) as f32;
        rms.push(s.sqrt());
    }
    levels_to_peaks(&rms)
}

/// Scales raw levels (any positive numbers) to 0..=255 relative to the loudest, with a square-root curve so quiet
/// speech still shows in the waveform.
pub fn levels_to_peaks(levels: &[f32]) -> Vec<u8> {
    let max = levels.iter().cloned().fold(0.0f32, f32::max);
    if max <= 1e-6 {
        return vec![0; levels.len()];
    }
    levels.iter().map(|l| ((l / max).sqrt() * 255.0).round().clamp(0.0, 255.0) as u8).collect()
}

/// Reduces a long list of levels to at most 64 values (the server limit) by taking the max of each group.
pub fn reduce_levels(levels: &[f32], n: usize) -> Vec<f32> {
    if levels.len() <= n || n == 0 {
        return levels.to_vec();
    }
    (0..n)
        .map(|i| {
            let a = i * levels.len() / n;
            let b = ((i + 1) * levels.len() / n).max(a + 1).min(levels.len());
            levels[a..b].iter().cloned().fold(0.0f32, f32::max)
        })
        .collect()
}

// ---------------------------------------------------------------- Ogg Opus encoding

/// Streaming Ogg Opus writer for mono 48 kHz audio: push samples as they are captured, call `finish` at the end.
pub struct OggOpusWriter<W: Write> {
    enc: opus::Encoder,
    pw: PacketWriter<'static, W>,
    serial: u32,
    pending: Vec<f32>,
    pre_skip: u32,
    granule: u64,
    /// Samples the caller pushed (without padding).
    pub total: u64,
    buf: Vec<u8>,
    held: Option<Vec<u8>>,
}

impl<W: Write> OggOpusWriter<W> {
    pub fn new(out: W, bitrate: i32) -> Result<Self> {
        let mut enc = opus::Encoder::new(OPUS_RATE, opus::Channels::Mono, opus::Application::Voip).map_err(audio_err)?;
        enc.set_bitrate(opus::Bitrate::Bits(bitrate)).map_err(audio_err)?;
        let pre_skip = enc.get_lookahead().unwrap_or(312).max(0) as u32;
        let serial = rand::random::<u32>();
        let mut pw = PacketWriter::new(out);
        let mut head = Vec::with_capacity(19);
        head.extend_from_slice(b"OpusHead");
        head.push(1); // version
        head.push(1); // channels
        head.extend_from_slice(&(pre_skip as u16).to_le_bytes());
        head.extend_from_slice(&OPUS_RATE.to_le_bytes());
        head.extend_from_slice(&0i16.to_le_bytes()); // gain
        head.push(0); // mapping family
        pw.write_packet(head, serial, PacketWriteEndInfo::EndPage, 0).map_err(audio_err)?;
        let vendor = b"WorkChat";
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0u32.to_le_bytes());
        pw.write_packet(tags, serial, PacketWriteEndInfo::EndPage, 0).map_err(audio_err)?;
        Ok(OggOpusWriter { enc, pw, serial, pending: Vec::new(), pre_skip, granule: pre_skip as u64, total: 0, buf: vec![0u8; 4000], held: None })
    }

    fn encode_frame(&mut self, frame: &[f32], last: bool, real: usize) -> Result<()> {
        let n = self.enc.encode_float(frame, &mut self.buf).map_err(audio_err)?;
        let pkt = self.buf[..n].to_vec();
        self.granule += FRAME as u64;
        // Packets are held back one step so the final one can carry the end-of-stream flag.
        if let Some(prev) = self.held.replace(pkt) {
            let gp = self.granule - FRAME as u64;
            self.pw.write_packet(prev, self.serial, PacketWriteEndInfo::NormalPacket, gp).map_err(audio_err)?;
        }
        if last {
            let gp = self.pre_skip as u64 + self.total;
            let held = self.held.take().unwrap_or_default();
            let _ = real;
            self.pw.write_packet(held, self.serial, PacketWriteEndInfo::EndStream, gp).map_err(audio_err)?;
        }
        Ok(())
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<()> {
        self.total += samples.len() as u64;
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= FRAME {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            self.encode_frame(&frame, false, FRAME)?;
        }
        Ok(())
    }

    /// Flushes the last partial frame (zero padded) and ends the stream. Returns the writer's output.
    pub fn finish(mut self) -> Result<()> {
        let real = self.pending.len();
        let mut frame = std::mem::take(&mut self.pending);
        frame.resize(FRAME, 0.0);
        self.encode_frame(&frame, true, real)
    }
}

/// Encodes a whole mono buffer (any rate) to Ogg Opus bytes.
pub fn encode_ogg_opus(pcm: &Pcm, bitrate: i32) -> Result<Vec<u8>> {
    let samples = resample(&pcm.samples, pcm.rate, OPUS_RATE);
    let mut out = Vec::new();
    let mut w = OggOpusWriter::new(&mut out, bitrate)?;
    w.push(&samples)?;
    w.finish()?;
    Ok(out)
}

// ---------------------------------------------------------------- decoding

fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved.chunks_exact(channels).map(|c| c.iter().sum::<f32>() / channels as f32).collect()
}

/// Decodes an Ogg Opus file (mono or stereo, downmixed) with libopus.
pub fn decode_ogg_opus(data: &[u8]) -> Result<Pcm> {
    let mut rd = ogg::reading::PacketReader::new(std::io::Cursor::new(data));
    let head = rd.read_packet().map_err(audio_err)?.ok_or_else(|| audio_err("empty file"))?;
    if head.data.len() < 19 || &head.data[..8] != b"OpusHead" {
        return Err(audio_err("not an Ogg Opus file"));
    }
    let channels = head.data[9].max(1) as usize;
    let pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]) as usize;
    let ch = if channels >= 2 { opus::Channels::Stereo } else { opus::Channels::Mono };
    let mut dec = opus::Decoder::new(OPUS_RATE, ch).map_err(audio_err)?;
    let _tags = rd.read_packet().map_err(audio_err)?;
    let per = if channels >= 2 { 2 } else { 1 };
    let mut out: Vec<f32> = Vec::new();
    let mut buf = vec![0f32; 5760 * per];
    let mut end_granule: Option<u64> = None;
    while let Some(p) = rd.read_packet().map_err(audio_err)? {
        if p.last_in_stream() {
            end_granule = Some(p.absgp_page());
        }
        let n = dec.decode_float(&p.data, &mut buf, false).map_err(audio_err)?;
        out.extend_from_slice(&downmix(&buf[..n * per], per));
    }
    let skip = pre_skip.min(out.len());
    out.drain(..skip);
    if let Some(g) = end_granule {
        let want = g.saturating_sub(pre_skip as u64) as usize;
        if want < out.len() {
            out.truncate(want);
        }
    }
    Ok(Pcm { rate: OPUS_RATE, samples: out })
}

pub fn is_ogg_opus(data: &[u8]) -> bool {
    data.len() > 36 && &data[..4] == b"OggS" && data[28..].windows(8).take(40).any(|w| w == b"OpusHead")
}

/// Decodes any supported audio file to mono PCM. `max_seconds` guards memory for very long files.
pub fn decode_file(path: &Path, max_seconds: u32) -> Result<Pcm> {
    let data = std::fs::read(path)?;
    let pcm = if is_ogg_opus(&data) { decode_ogg_opus(&data)? } else { decode_symphonia(data, path.extension().and_then(|e| e.to_str()))? };
    if pcm.rate > 0 && pcm.samples.len() as u64 > max_seconds as u64 * pcm.rate as u64 {
        return Err(CoreError::local("This audio is too long to play inside the app. Use Open instead."));
    }
    Ok(pcm)
}

/// Duration without decoding: Ogg Opus from the last page's granule position, other formats from the container.
pub fn probe_duration_ms(path: &Path) -> Option<u32> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let mut head = [0u8; 64];
    let mut f = std::fs::File::open(path).ok()?;
    use std::io::{Read, Seek, SeekFrom};
    let n = f.read(&mut head).ok()?;
    if n > 36 && &head[..4] == b"OggS" && head[28..n].windows(8).any(|w| w == b"OpusHead") {
        let len = f.metadata().ok()?.len();
        let tail = len.min(65_536);
        f.seek(SeekFrom::Start(len - tail)).ok()?;
        let mut buf = vec![0u8; tail as usize];
        f.read_exact(&mut buf).ok()?;
        let p = buf.windows(4).rposition(|w| w == b"OggS")?;
        let g = u64::from_le_bytes(buf.get(p + 6..p + 14)?.try_into().ok()?);
        let pre = u16::from_le_bytes([head[28 + 10], head[28 + 11]]) as u64;
        return Some((g.saturating_sub(pre) * 1000 / OPUS_RATE as u64) as u32);
    }
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(e) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(e);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).ok()?;
    let t = probed.format.default_track()?;
    let (rate, frames) = (t.codec_params.sample_rate?, t.codec_params.n_frames?);
    Some((frames * 1000 / rate as u64) as u32)
}

fn decode_symphonia(data: Vec<u8>, ext: Option<&str>) -> Result<Pcm> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as SErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(data)), Default::default());
    let mut hint = Hint::new();
    if let Some(e) = ext {
        hint.with_extension(e);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(|_| audio_err("this audio format is not supported"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or_else(|| audio_err("no audio track"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|_| audio_err("this audio codec is not supported"))?;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut out: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SErr::IoError(_)) | Err(SErr::ResetRequired) => break,
            Err(e) => return Err(audio_err(e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = *buf.spec();
                rate = spec.rate;
                let mut sb = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
                sb.copy_interleaved_ref(buf);
                out.extend_from_slice(&downmix(sb.samples(), spec.channels.count()));
            }
            Err(SErr::DecodeError(_)) => continue,
            Err(SErr::IoError(_)) => break,
            Err(e) => return Err(audio_err(e)),
        }
    }
    if out.is_empty() || rate == 0 {
        return Err(audio_err("no audio could be decoded"));
    }
    Ok(Pcm { rate, samples: out })
}

// ---------------------------------------------------------------- time stretching for 1.5x / 2x playback

/// WSOLA time stretcher: plays mono audio faster or slower without changing the pitch.
pub struct Stretcher {
    input: std::sync::Arc<Vec<f32>>,
    win: usize,
    hop: usize,
    window: Vec<f32>,
    /// Position in the input where the next analysis frame is expected (nominal, fractional).
    pub pos: f64,
    pub speed: f64,
    prev_start: Option<isize>,
    overlap: Vec<f32>,
    ready: std::collections::VecDeque<f32>,
}

impl Stretcher {
    pub fn new(input: std::sync::Arc<Vec<f32>>, rate: u32) -> Self {
        let win = ((rate as usize * 40 / 1000) / 2 * 2).max(64);
        let hop = win / 2;
        let window = (0..win).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / win as f32).cos()).collect();
        Stretcher { input, win, hop, window, pos: 0.0, speed: 1.0, prev_start: None, overlap: vec![0.0; hop], ready: Default::default() }
    }

    pub fn len(&self) -> usize {
        self.input.len()
    }

    pub fn seek(&mut self, sample: usize) {
        self.pos = sample.min(self.input.len()) as f64;
        self.prev_start = None;
        self.overlap.iter_mut().for_each(|x| *x = 0.0);
        self.ready.clear();
    }

    pub fn finished(&self) -> bool {
        self.pos >= self.input.len() as f64 && self.ready.is_empty()
    }

    fn at(&self, i: isize) -> f32 {
        if i < 0 {
            0.0
        } else {
            *self.input.get(i as usize).unwrap_or(&0.0)
        }
    }

    fn fill(&mut self) {
        let (win, hop) = (self.win, self.hop);
        let nominal = self.pos as isize;
        let mut start = nominal;
        if (self.speed - 1.0).abs() < 1e-3 {
            // Plain playback: frames are consecutive, no search needed.
            if let Some(p) = self.prev_start {
                start = p + hop as isize;
            }
        } else if let Some(p) = self.prev_start {
            let target = p + hop as isize;
            let radius = (hop / 3) as isize;
            let mut best = (f32::MIN, nominal);
            let mut d = -radius;
            while d <= radius {
                let cand = nominal + d;
                let mut c = 0.0f32;
                let mut j = 0;
                while j < hop {
                    c += self.at(cand + j as isize) * self.at(target + j as isize);
                    j += 2;
                }
                if c > best.0 {
                    best = (c, cand);
                }
                d += 2;
            }
            start = best.1;
        }
        let mut frame = vec![0f32; win];
        for (i, f) in frame.iter_mut().enumerate() {
            *f = self.at(start + i as isize) * self.window[i];
        }
        for i in 0..hop {
            self.ready.push_back(self.overlap[i] + frame[i]);
        }
        self.overlap.copy_from_slice(&frame[hop..]);
        self.prev_start = Some(start);
        if (self.speed - 1.0).abs() < 1e-3 {
            self.pos = (start + hop as isize) as f64;
        } else {
            self.pos += hop as f64 * self.speed;
        }
    }

    /// Fills `out` with the next samples (zeros after the end).
    pub fn read(&mut self, out: &mut [f32]) {
        for o in out.iter_mut() {
            if self.ready.is_empty() {
                if self.pos >= self.input.len() as f64 {
                    *o = 0.0;
                    continue;
                }
                self.fill();
            }
            *o = self.ready.pop_front().unwrap_or(0.0);
        }
    }

    /// Current playback position in input samples (what the listener hears now, roughly).
    pub fn position(&self) -> usize {
        (self.pos as usize).min(self.input.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(n: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(n)
    }

    fn tone(rate: u32, secs: f32, hz: f32) -> Pcm {
        let n = (rate as f32 * secs) as usize;
        Pcm { rate, samples: (0..n).map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin()).collect() }
    }

    fn dominant_hz(p: &Pcm) -> f32 {
        // zero crossings of the middle part
        let s = &p.samples[p.samples.len() / 4..p.samples.len() * 3 / 4];
        let z = s.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        z / (s.len() as f32 / p.rate as f32)
    }

    #[test]
    fn decodes_seeded_style_files() {
        for (name, rate_ok) in [("tone.ogg", 48_000u32), ("tone.m4a", 48_000), ("tone.mp3", 48_000), ("tone.wav", 8_000)] {
            let p = decode_file(&fixture(name), 600).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(p.rate, rate_ok, "{name}");
            let d = p.duration_ms();
            assert!((1350..=1750).contains(&d), "{name} duration {d}");
            let hz = dominant_hz(&p);
            assert!((hz - 440.0).abs() < 25.0, "{name} frequency {hz}");
        }
    }

    #[test]
    fn opus_roundtrip_keeps_duration_and_tone() {
        let src = tone(48_000, 2.0, 600.0);
        let bytes = encode_ogg_opus(&src, 24_000).unwrap();
        assert!(bytes.starts_with(b"OggS"));
        assert!(is_ogg_opus(&bytes));
        assert!(bytes.len() < 20_000, "24 kbit/s speech-sized: {}", bytes.len());
        let back = decode_ogg_opus(&bytes).unwrap();
        assert_eq!(back.rate, 48_000);
        let d = back.duration_ms() as i32;
        assert!((d - 2000).abs() <= 25, "duration {d}");
        assert!((dominant_hz(&back) - 600.0).abs() < 30.0);
    }

    #[test]
    fn encoded_file_is_valid_for_ffprobe_like_readers() {
        // Headers: page 1 OpusHead, page 2 OpusTags, then audio; last page has the EOS flag (0x04).
        let bytes = encode_ogg_opus(&tone(16_000, 0.5, 300.0), 16_000).unwrap();
        assert_eq!(&bytes[28..36], b"OpusHead");
        let last = bytes.windows(4).rposition(|w| w == b"OggS").unwrap();
        assert_eq!(bytes[last + 5] & 0x04, 0x04);
    }

    #[test]
    fn streaming_writer_matches_buffered() {
        let src = tone(48_000, 1.0, 500.0);
        let mut out = Vec::new();
        let mut w = OggOpusWriter::new(&mut out, 24_000).unwrap();
        for chunk in src.samples.chunks(441) {
            w.push(chunk).unwrap();
        }
        w.finish().unwrap();
        let back = decode_ogg_opus(&out).unwrap();
        assert!((back.duration_ms() as i32 - 1000).abs() <= 25);
    }

    #[test]
    fn peaks_have_requested_shape() {
        let mut s = vec![0.0f32; 4800];
        for (i, x) in s.iter_mut().enumerate().skip(2400) {
            *x = 0.8 * ((i as f32) * 0.3).sin();
        }
        let p = peaks(&s, 64);
        assert_eq!(p.len(), 64);
        assert!(p[..30].iter().all(|v| *v < 5));
        assert!(p[40..].iter().all(|v| *v > 200));
        assert_eq!(peaks(&[0.0; 100], 8), vec![0u8; 8]);
        assert!(peaks(&[], 8).is_empty());
        assert_eq!(reduce_levels(&vec![1.0; 200], 64).len(), 64);
    }

    #[test]
    fn resample_changes_length() {
        let r = resample(&vec![0.1; 44_100], 44_100, 48_000);
        assert!((r.len() as i32 - 48_000).abs() <= 1);
        assert_eq!(resample(&[1.0, 2.0], 8000, 8000), vec![1.0, 2.0]);
    }

    #[test]
    fn stretcher_speeds_up_without_changing_pitch() {
        let src = tone(16_000, 4.0, 400.0);
        let mut st = Stretcher::new(std::sync::Arc::new(src.samples.clone()), 16_000);
        st.speed = 2.0;
        let mut out = Vec::new();
        let mut buf = vec![0f32; 1024];
        while !st.finished() {
            st.read(&mut buf);
            out.extend_from_slice(&buf);
        }
        // 4 s at 2x is about 2 s.
        let secs = out.len() as f32 / 16_000.0;
        assert!((secs - 2.0).abs() < 0.2, "{secs}");
        let p = Pcm { rate: 16_000, samples: out };
        assert!((dominant_hz(&p) - 400.0).abs() < 40.0, "{}", dominant_hz(&p));
    }

    #[test]
    fn stretcher_at_normal_speed_is_transparent() {
        let src = tone(16_000, 1.0, 400.0);
        let mut st = Stretcher::new(std::sync::Arc::new(src.samples.clone()), 16_000);
        let mut out = vec![0f32; 8000];
        st.read(&mut out);
        // After the first half-window of ramp-up the output equals the input.
        for i in 1000..7000 {
            assert!((out[i] - src.samples[i]).abs() < 1e-3, "{i}");
        }
        st.seek(0);
        assert_eq!(st.position(), 0);
    }
}
