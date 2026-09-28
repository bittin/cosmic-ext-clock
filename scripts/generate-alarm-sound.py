#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only

import math
import struct
import wave
from pathlib import Path

SAMPLE_RATE = 48_000
DURATION_SECONDS = 4.0
NOTES = (
    (0.00, 0.72, 880.00),
    (0.82, 0.72, 1318.51),
    (1.64, 0.90, 1108.73),
    (2.65, 0.48, 880.00),
    (3.20, 0.48, 1318.51),
)


def sample_at(time: float) -> float:
    value = 0.0
    for start, length, frequency in NOTES:
        local = time - start
        if not 0.0 <= local < length:
            continue
        attack = min(1.0, local / 0.025)
        release = min(1.0, (length - local) / 0.14)
        envelope = attack * release * math.exp(-1.6 * local)
        fundamental = math.sin(2.0 * math.pi * frequency * local)
        harmonic = 0.22 * math.sin(2.0 * math.pi * frequency * 2.0 * local)
        shimmer = 0.08 * math.sin(2.0 * math.pi * frequency * 3.0 * local)
        value += envelope * (fundamental + harmonic + shimmer)
    return max(-1.0, min(1.0, value * 0.34))


def main() -> None:
    output = Path(__file__).resolve().parents[1] / "resources/sounds/alarm.wav"
    output.parent.mkdir(parents=True, exist_ok=True)
    frame_count = int(SAMPLE_RATE * DURATION_SECONDS)
    payload = bytearray()
    for frame in range(frame_count):
        payload.extend(struct.pack("<h", round(sample_at(frame / SAMPLE_RATE) * 32767)))

    with wave.open(str(output), "wb") as alarm:
        alarm.setnchannels(1)
        alarm.setsampwidth(2)
        alarm.setframerate(SAMPLE_RATE)
        alarm.writeframes(payload)

    print(f"generated {output.relative_to(output.parents[2])}")


if __name__ == "__main__":
    main()
