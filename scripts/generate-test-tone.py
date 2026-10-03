"""Synthetic PCM fixture only: writes a tone, never opens a microphone or output."""
import math
from pathlib import Path
import struct
import wave

output = Path(__file__).resolve().parents[1] / "artifacts/test-tone.wav"
output.parent.mkdir(parents=True, exist_ok=True)
with wave.open(str(output), "wb") as wav:
    wav.setnchannels(2)
    wav.setsampwidth(2)
    wav.setframerate(48000)
    for second in range(30):
        pcm = b"".join(
            struct.pack("<hh", *([int(4000 * math.sin(2 * math.pi * 440 * frame / 48000))] * 2))
            for frame in range(48000)
        )
        wav.writeframes(pcm)
