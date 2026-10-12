/** Sample rate Whisper expects. */
export const TARGET_SAMPLE_RATE = 16000;

/**
 * Downsample mono PCM to `toRate` by averaging the input samples that fall in
 * each output sample's span (a box filter, which also limits aliasing).
 * Returns the input unchanged when no downsampling is needed.
 */
export function downsample(input: Float32Array, fromRate: number, toRate = TARGET_SAMPLE_RATE): Float32Array {
  if (fromRate <= toRate) return input;
  const ratio = fromRate / toRate;
  const outLength = Math.floor(input.length / ratio);
  const output = new Float32Array(outLength);
  for (let i = 0; i < outLength; i++) {
    const start = Math.floor(i * ratio);
    const end = Math.min(input.length, Math.floor((i + 1) * ratio));
    let sum = 0;
    for (let j = start; j < end; j++) sum += input[j];
    output[i] = end > start ? sum / (end - start) : 0;
  }
  return output;
}

/** Join recorded chunks into one buffer. */
export function concatChunks(chunks: Float32Array[]): Float32Array {
  const total = chunks.reduce((n, c) => n + c.length, 0);
  const out = new Float32Array(total);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.length;
  }
  return out;
}

export interface MicRecording {
  /** Stop recording and return 16 kHz mono PCM. */
  stop: () => Promise<Float32Array>;
  /** Stop recording and discard the audio. */
  cancel: () => void;
}

/** Start recording the default microphone. Rejects if access is denied. */
export async function startMicRecording(): Promise<MicRecording> {
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
  });
  const context = new AudioContext();
  const source = context.createMediaStreamSource(stream);
  // ScriptProcessorNode is deprecated but works in every WebView Tauri uses,
  // without loading a separate AudioWorklet module.
  const processor = context.createScriptProcessor(4096, 1, 1);
  const chunks: Float32Array[] = [];
  processor.onaudioprocess = (e) => {
    chunks.push(new Float32Array(e.inputBuffer.getChannelData(0)));
  };
  source.connect(processor);
  // Must reach the destination for onaudioprocess to fire; its output is silent.
  processor.connect(context.destination);

  const release = () => {
    processor.disconnect();
    source.disconnect();
    stream.getTracks().forEach((t) => t.stop());
    void context.close();
  };

  return {
    stop: async () => {
      const rate = context.sampleRate;
      release();
      return downsample(concatChunks(chunks), rate);
    },
    cancel: release,
  };
}
