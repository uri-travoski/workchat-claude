"""Short H.264/AAC videos and synthetic voice notes made with ffmpeg and a little maths (no recordings, no downloads)."""
import base64
import math
import random
import shutil
import struct
import subprocess
import wave
from pathlib import Path

FF = shutil.which("ffmpeg") or "/usr/bin/ffmpeg"
FONT = "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf"


def run(args):
    r = subprocess.run([FF, "-hide_banner", "-loglevel", "error", "-y", *args], capture_output=True, text=True)
    if r.returncode:
        raise RuntimeError("ffmpeg failed: " + r.stderr[-600:])


VIDEOS = {
    # name: (video source, size, seconds, caption, audio frequency)
    "office-walkthrough.mp4": ("gradients=s={w}x{h}:d={d}:speed=0.015:c0=0x21375b:c1=0x1e5bb8:c2=0x19c37d:c3=0xf5a524:nb_colors=4:type=spiral", (640, 360), 8, "Office walkthrough", 330),
    "site-visit-portrait.mp4": ("mandelbrot=s={w}x{h}:rate=24:end_scale=0.05:maxiter=120", (360, 640), 6, "Site visit", 392),
    "product-demo.mp4": ("testsrc2=s={w}x{h}:rate=24:d={d}", (480, 480), 6, "Product demo v2.4", 262),
    "townhall-teaser.mp4": ("life=s={w}x{h}:mold=12:r=24:ratio=0.08:death_color=#19273f:life_color=#19c37d:stitch=1", (640, 360), 8, "Town Hall teaser", 220),
}


def make_video(outdir: Path, name: str):
    src, (w, h), d, caption, freq = VIDEOS[name]
    vf = (
        f"drawbox=x=0:y=ih-90:w=iw:h=90:color=black@0.45:t=fill,"
        f"drawtext=fontfile={FONT}:text='{caption}':fontcolor=white:fontsize={max(22, w // 18)}:x=(w-text_w)/2:y=h-62,"
        f"format=yuv420p"
    )
    out = outdir / name
    run([
        "-f", "lavfi", "-i", src.format(w=w, h=h, d=d),
        "-f", "lavfi", "-i", f"sine=frequency={freq}:duration={d},tremolo=f=3:d=0.6,volume=0.25",
        "-t", str(d), "-vf", vf,
        "-c:v", "libx264", "-profile:v", "main", "-preset", "medium", "-crf", "31", "-maxrate", "700k", "-bufsize", "1400k", "-g", "48",
        "-c:a", "aac", "-b:a", "48k", "-ac", "1", "-movflags", "+faststart", str(out),
    ])
    if out.stat().st_size > 1_900_000:
        raise RuntimeError(f"{name} is larger than 2 MB")
    return out


def poster_b64(video: Path, width=240):
    tmp = video.with_suffix(".poster.jpg")
    run(["-ss", "0.5", "-i", str(video), "-frames:v", "1", "-vf", f"scale={width}:-2", "-q:v", "6", str(tmp)])
    data = tmp.read_bytes()
    tmp.unlink()
    return base64.b64encode(data).decode(), len(data)


def probe(path: Path):
    """(width, height, duration_ms) via ffprobe-less ffmpeg parsing."""
    r = subprocess.run([FF, "-hide_banner", "-i", str(path)], capture_output=True, text=True)
    import re
    dur = re.search(r"Duration: (\d+):(\d+):(\d+\.\d+)", r.stderr)
    size = re.search(r"Video:.* (\d{2,5})x(\d{2,5})", r.stderr)
    ms = int((int(dur.group(1)) * 3600 + int(dur.group(2)) * 60 + float(dur.group(3))) * 1000) if dur else None
    return (int(size.group(1)), int(size.group(2)), ms) if size else (None, None, ms)


# ------------------------------------------------------------------ voice notes

def synth_speech(path_wav: Path, seconds: float, seed: int, base_hz: float):
    """A speech-like signal: syllable bursts with pitch glides, vowel-ish harmonics and breath noise, with pauses between phrases."""
    rate = 16000
    rnd = random.Random(seed)
    n = int(seconds * rate)
    out = [0.0] * n
    t = 0.0
    while t < seconds - 0.2:
        phrase = rnd.uniform(1.0, 2.4)
        syl = rnd.randint(3, 8)
        sd = phrase / syl
        f0 = base_hz * rnd.uniform(0.9, 1.15)
        for s in range(syl):
            st = t + s * sd
            f_end = f0 * rnd.uniform(0.85, 1.2)
            vowel = rnd.choice([(700, 1200), (400, 2000), (300, 800), (500, 1500)])
            i0, i1 = int(st * rate), min(n, int((st + sd * 0.82) * rate))
            ph = 0.0
            for i in range(i0, i1):
                u = (i - i0) / max(1, i1 - i0)
                env = math.sin(math.pi * u) ** 1.4
                f = f0 + (f_end - f0) * u
                ph += 2 * math.pi * f / rate
                v = 0.0
                for k in range(1, 9):
                    fk = f * k
                    gain = math.exp(-((fk - vowel[0]) / 380) ** 2) + 0.7 * math.exp(-((fk - vowel[1]) / 500) ** 2) + 0.08 / k
                    v += gain * math.sin(ph * k)
                out[i] += 0.16 * env * v + 0.03 * env * rnd.uniform(-1, 1)
            f0 = f_end
        t += phrase + rnd.uniform(0.25, 0.8)
    with wave.open(str(path_wav), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(b"".join(struct.pack("<h", max(-32000, min(32000, int(x * 22000)))) for x in out))


def make_voice(outdir: Path, name: str, seconds: float, seed: int, base_hz: float):
    wav = outdir / (name + ".tmp.wav")
    synth_speech(wav, seconds, seed, base_hz)
    out = outdir / name
    if name.endswith(".ogg"):
        run(["-i", str(wav), "-c:a", "libopus", "-b:a", "24k", "-ac", "1", "-application", "voip", str(out)])
    else:
        run(["-i", str(wav), "-c:a", "aac", "-b:a", "32k", "-ac", "1", "-movflags", "+faststart", "-f", "mp4", str(out)])
    wav.unlink()
    return out


def peaks_of(path: Path, buckets=64):
    r = subprocess.run([FF, "-hide_banner", "-loglevel", "error", "-i", str(path), "-f", "s16le", "-ac", "1", "-ar", "8000", "-"], capture_output=True)
    data = r.stdout
    samples = struct.unpack("<%dh" % (len(data) // 2), data[: len(data) // 2 * 2])
    if not samples:
        return [0] * buckets
    step = max(1, len(samples) // buckets)
    rms = []
    for b in range(buckets):
        seg = samples[b * step:(b + 1) * step] or [0]
        rms.append(math.sqrt(sum(x * x for x in seg) / len(seg)))
    top = max(rms) or 1.0
    return [min(255, int(255 * (v / top) ** 0.8)) for v in rms]


VOICE_NOTES = {
    # name: (seconds, seed, base pitch)
    "voice-0001.m4a": (6.0, 1, 130),
    "voice-0002.m4a": (11.0, 2, 210),
    "voice-0003.m4a": (18.0, 3, 120),
    "voice-0004.ogg": (9.0, 4, 190),
    "voice-0005.m4a": (27.0, 5, 150),
    "voice-0006.m4a": (4.0, 6, 230),
    "voice-0007.ogg": (42.0, 7, 125),
    "voice-0008.m4a": (14.0, 8, 175),
}
