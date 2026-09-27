"""用代码合成第四版配乐：读 data/timeline.js，按场景的小节与段落编配，输出 music.wav（44.1 kHz 立体声）。

编配：氛围 pad（和弦 Am–F–C–G）、低音、克制的鼓、琶音；痛点段滤波上升，揭晓处一次重拍，
能力段稳定律动，效果段加厚，结尾收住。所有音色由加法合成与噪声生成，混响用合成冲激响应做 FFT 卷积。
只依赖 numpy。
"""
import json
import re
import sys
import wave
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
SR = 44100
rng = np.random.default_rng(7)

tl = json.loads(re.search(r"=\s*(\{.*\})\s*;", (HERE / "data/timeline.js").read_text(), re.S).group(1))
BPM, BPB = tl["bpm"], tl["beatsPerBar"]
BEAT = 60 / BPM
BAR = BEAT * BPB
bars = []  # per-bar section name
for s in tl["scenes"]:
    bars += [s["music"]] * s["bars"]
NB = len(bars)
TAIL = 3.0
N = int((NB * BAR + TAIL) * SR)
L = np.zeros(N); R = np.zeros(N)
dry_send = np.zeros(N)  # mono send to reverb


def midi(m):
    return 440 * 2 ** ((m - 69) / 12)


# Am – F – C – G, one chord per bar (root midi, triad offsets)
PROG = [(57, (0, 3, 7)), (53, (0, 4, 7)), (60, (0, 4, 7)), (55, (0, 4, 7))]


def env(n, a, d, sustain=1.0, r=0.0):
    """attack/decay-to-sustain envelope over n samples, with release r seconds at the end."""
    t = np.arange(n) / SR
    e = np.minimum(1, t / max(a, 1e-4))
    if d > 0:
        e = np.where(t > a, sustain + (1 - sustain) * np.exp(-(t - a) / d), e)
    if r > 0:
        e *= np.clip((n / SR - t) / r, 0, 1)
    return e


def add(sig, start_s, gain=1.0, pan=0.0, send=0.0):
    i = int(start_s * SR)
    if i >= N:
        return
    sig = sig[: N - i]
    lg, rg = np.cos((pan + 1) * np.pi / 4), np.sin((pan + 1) * np.pi / 4)
    L[i:i + len(sig)] += sig * gain * lg * 1.414
    R[i:i + len(sig)] += sig * gain * rg * 1.414
    dry_send[i:i + len(sig)] += sig * gain * send


def additive(freq, dur, harmonics=8, tilt=1.0, detune=0.0, decay_h=0.0):
    """band-limited saw-like tone; decay_h > 0 makes upper harmonics die faster (pluck)."""
    n = int(dur * SR)
    t = np.arange(n) / SR
    out = np.zeros(n)
    for h in range(1, harmonics + 1):
        f = freq * h * (1 + detune)
        if f > SR / 2.2:
            break
        amp = 1 / h ** tilt
        if decay_h:
            amp = amp * np.exp(-t * decay_h * h)
        out += amp * np.sin(2 * np.pi * f * t + h)
    return out


SECTION_LEVEL = {"intro": .85, "tension": .75, "reveal": .9, "groove": .8, "lift": 1.0, "outro": .8}

# ---- pad ----
for b in range(NB):
    sec = bars[b]
    root, tri = PROG[b % 4]
    lvl = SECTION_LEVEL[sec]
    bright = {"intro": 1.8, "tension": 1.3 + .1 * (b % 6), "reveal": 1.1, "groove": 1.3, "lift": 1.0, "outro": 1.9}[sec]
    dur = BAR + 1.2
    for k, off in enumerate(tri + (12,)):
        f = midi(root + off)
        tone = sum(additive(f, dur, 10, bright, d) for d in (-0.004, 0, 0.004)) / 3
        tone *= env(len(tone), .6, 0, 1, 1.1)
        add(tone, b * BAR, .07 * lvl, pan=(-.4, .4, -.2, .2)[k], send=.5)

# ---- bass ----
for b in range(NB):
    sec = bars[b]
    if sec in ("intro",):
        continue
    root = PROG[b % 4][0] - 24
    f = midi(root)
    steps = 8 if sec in ("tension", "groove", "lift") else 2
    for s in range(steps):
        t0 = b * BAR + s * BAR / steps
        d = BAR / steps * .9
        tone = additive(f, d, 5, 1.6) * env(int(d * SR), .005, .18, .35, .03)
        tone += np.sin(2 * np.pi * f / 2 * np.arange(int(d * SR)) / SR) * env(int(d * SR), .005, .3, .5, .03) * .8
        acc = 1.0 if s % 2 == 0 else .7
        add(tone, t0, .16 * SECTION_LEVEL[sec] * acc)


# ---- drums ----
def kick(gain=1.0):
    n = int(.45 * SR); t = np.arange(n) / SR
    f = 45 + 95 * np.exp(-t * 28)
    ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * np.exp(-t * 7) * gain


def noise_hit(dur, decay, hp=True):
    n = int(dur * SR)
    x = rng.standard_normal(n)
    if hp:
        x = np.diff(np.concatenate([[0], x]))  # crude high-pass
    return x * np.exp(-np.arange(n) / SR * decay)


for b in range(NB):
    sec = bars[b]
    t0 = b * BAR
    if sec in ("groove", "lift"):
        for q in range(4):
            if sec == "lift" or q in (0, 2):
                add(kick(), t0 + q * BEAT, .55)
            if q in (1, 3):
                add(noise_hit(.25, 18) * .6 + additive(190, .25, 3) * env(int(.25 * SR), .001, .05, 0) * .5, t0 + q * BEAT, .22, send=.3)
        for e in range(8 if sec == "groove" else 16):
            step = BAR / (8 if sec == "groove" else 16)
            add(noise_hit(.05, 90), t0 + e * step, .05 * (1 if e % 2 else .6), pan=.3)
    if sec == "tension":
        add(kick(.6), t0, .4)
        if b % 2:
            add(kick(.5), t0 + 2 * BEAT, .3)

# ---- riser into reveal, and big hits ----
reveal_bar = bars.index("reveal")
rise_d = 2 * BAR
n = int(rise_d * SR); t = np.arange(n) / SR
riser = rng.standard_normal(n)
riser = np.diff(np.concatenate([[0], riser])) * (t / rise_d) ** 2.2
riser += additive(220, rise_d, 6, 1.2) * (t / rise_d) ** 3 * .3
add(riser, (reveal_bar - 2) * BAR, .12, send=.4)


def big_hit(at, gain=1.0):
    add(kick(1.2), at, .8 * gain)
    add(noise_hit(2.8, 1.6, hp=True), at, .16 * gain, send=.8)
    add(additive(midi(45), 3, 8, 1.2) * env(int(3 * SR), .005, 1.2, 0), at, .25 * gain, send=.5)


big_hit(reveal_bar * BAR)
big_hit(bars.index("lift") * BAR, .8)
big_hit(bars.index("outro") * BAR, .9)

# ---- arp (pluck) ----
for b in range(NB):
    sec = bars[b]
    if sec not in ("groove", "lift", "reveal"):
        continue
    root, tri = PROG[b % 4]
    pattern = [0, 1, 2, 3, 2, 1, 2, 3] * 2
    notes = [root + 12 + off for off in tri] + [root + 24]
    for s in range(16):
        f = midi(notes[pattern[s]])
        d = BEAT / 4 * 1.6
        tone = additive(f, d, 9, 1.0, decay_h=2.2) * env(int(d * SR), .002, .12, 0)
        add(tone, b * BAR + s * BEAT / 4, .05 * SECTION_LEVEL[sec], pan=(-.5 if s % 2 else .5), send=.45)

# ---- reverb: synthetic impulse response, FFT convolution ----
ir_n = int(2.6 * SR); ti = np.arange(ir_n) / SR
irL = rng.standard_normal(ir_n) * np.exp(-ti * 2.6); irR = rng.standard_normal(ir_n) * np.exp(-ti * 2.6)
irL[: int(.02 * SR)] = 0; irR[: int(.027 * SR)] = 0
sz = 1 << int(np.ceil(np.log2(N + ir_n)))
S = np.fft.rfft(dry_send, sz)
wetL = np.fft.irfft(S * np.fft.rfft(irL, sz), sz)[:N]
wetR = np.fft.irfft(S * np.fft.rfft(irR, sz), sz)[:N]
wetL /= np.max(np.abs(wetL)) + 1e-9; wetR /= np.max(np.abs(wetR)) + 1e-9
L += wetL * .18; R += wetR * .18

# ---- master: fade out, soft clip, normalize ----
fade = int(TAIL * SR)
L[-fade:] *= np.linspace(1, 0, fade) ** 2; R[-fade:] *= np.linspace(1, 0, fade) ** 2
mix = np.stack([L, R], 1)
mix = np.tanh(mix * 1.2) / np.tanh(1.2)
mix *= 0.89 / np.max(np.abs(mix))  # peak -1 dBFS
pcm = (mix * 32767).astype(np.int16)
with wave.open(str(HERE / "music.wav"), "wb") as w:
    w.setnchannels(2); w.setsampwidth(2); w.setframerate(SR); w.writeframes(pcm.tobytes())

# numeric report: peak and per-section RMS (dBFS)
print(f"length {N / SR:.1f}s, bars {NB}, bar {BAR:.3f}s, peak {20 * np.log10(np.max(np.abs(mix))):.1f} dBFS")
for sec in dict.fromkeys(bars):
    idx = [i for i, s in enumerate(bars) if s == sec]
    a, b = int(idx[0] * BAR * SR), int((idx[-1] + 1) * BAR * SR)
    rms = np.sqrt(np.mean(mix[a:b] ** 2))
    print(f"  {sec:8s} bars {idx[0]:2d}-{idx[-1]:2d}  rms {20 * np.log10(rms):6.1f} dBFS")
