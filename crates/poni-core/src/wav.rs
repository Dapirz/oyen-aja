//! Audio as Meeting notes records it: 16 kHz mono 16-bit, written as WAV
//! ([`WavWriter`]) from whatever the devices give ([`Downmix`]: any rate,
//! channels and sample format). From Oyenza's IELTS coach (removed in
//! 382c41a), where it recorded the answers.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// The rate everything is recorded at: speech needs no more.
pub const RATE: u32 = 16_000;

pub fn wav_header(rate: u32, channels: u16, bits: u16, data_len: u32) -> [u8; 44] {
    let block = channels * bits / 8;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * block as u32).to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&bits.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

/// Writes a recording as it comes: 16 kHz mono 16-bit, the header's
/// sizes filled in at the end.
pub struct WavWriter {
    out: BufWriter<File>,
    path: PathBuf,
    samples: u64,
    buf: Vec<u8>,
}

impl WavWriter {
    pub fn create(path: &Path) -> io::Result<WavWriter> {
        let mut out = BufWriter::with_capacity(64 * 1024, File::create(path)?);
        out.write_all(&wav_header(RATE, 1, 16, 0))?;
        Ok(WavWriter { out, path: path.to_path_buf(), samples: 0, buf: Vec::new() })
    }

    /// Append samples.
    pub fn write(&mut self, pcm: &[i16]) -> io::Result<()> {
        if pcm.is_empty() {
            return Ok(());
        }
        self.buf.clear();
        self.buf.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
        self.out.write_all(&self.buf)?;
        self.samples += pcm.len() as u64;
        Ok(())
    }

    pub fn secs(&self) -> f32 {
        self.samples as f32 / RATE as f32
    }

    /// Fill in the sizes and close the file: its path and length in seconds.
    pub fn finish(self) -> io::Result<(PathBuf, f32)> {
        let secs = self.secs();
        let data = (self.samples * 2) as u32;
        let mut f = self.out.into_inner().map_err(|e| e.into_error())?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&wav_header(RATE, 1, 16, data))?;
        f.flush()?;
        Ok((self.path, secs))
    }
}

/// How the microphone's samples are stored (Windows' mix format, usually 32-bit float).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormat {
    F32,
    I16,
    /// 24-bit in 3 bytes.
    I24,
    I32,
}

impl SampleFormat {
    pub fn bytes(self) -> usize {
        match self {
            SampleFormat::I16 => 2,
            SampleFormat::I24 => 3,
            SampleFormat::F32 | SampleFormat::I32 => 4,
        }
    }

    fn read(self, b: &[u8]) -> f32 {
        match self {
            SampleFormat::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            SampleFormat::I16 => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            SampleFormat::I24 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            SampleFormat::I32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
        }
    }
}

/// A second-order low-pass (RBJ's cookbook), transposed direct form II.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn low_pass(rate: f32, cutoff: f32, q: f32) -> Biquad {
        let w = std::f32::consts::TAU * cutoff / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b1 = (1.0 - cos) / a0;
        Biquad { b0: b1 / 2.0, b1, b2: b1 / 2.0, a1: -2.0 * cos / a0, a2: (1.0 - alpha) / a0, z1: 0.0, z2: 0.0 }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Turns the microphone's frames (any rate, channels and sample format)
/// into 16 kHz mono 16-bit: the channels averaged, a 4th-order low-pass
/// under 8 kHz (so nothing above it folds back into the speech), then
/// linear interpolation to 16 kHz. It keeps its place across packets, so
/// packets of any size give the same samples.
#[derive(Clone, Debug)]
pub struct Downmix {
    format: SampleFormat,
    channels: usize,
    /// Source samples per output sample.
    step: f64,
    /// Where the next output sample sits, in source samples after `prev`.
    pos: f64,
    prev: f32,
    filters: Option<[Biquad; 2]>,
}

impl Downmix {
    pub fn new(rate: u32, channels: u16, format: SampleFormat) -> Downmix {
        let rate = rate.max(1);
        // Butterworth, 4th order: two stages with these Qs, cut at 0.45 × 16 kHz.
        let filters = (rate > RATE + 1000).then(|| [0.541_196, 1.306_563].map(|q| Biquad::low_pass(rate as f32, 0.45 * RATE as f32, q)));
        Downmix { format, channels: channels.max(1) as usize, step: rate as f64 / RATE as f64, pos: 1.0, prev: 0.0, filters }
    }

    /// Interleaved frames in (`bytes`), 16 kHz samples onto `out`. Returns
    /// the loudest of them, 0..1 (the level meter).
    pub fn push(&mut self, bytes: &[u8], out: &mut Vec<i16>) -> f32 {
        let (size, channels) = (self.format.bytes(), self.channels);
        let start = out.len();
        for frame in bytes.chunks_exact(size * channels) {
            let sum: f32 = frame.chunks_exact(size).map(|s| self.format.read(s)).sum();
            self.feed(sum / channels as f32, out);
        }
        peak(&out[start..])
    }

    /// `frames` frames of silence (a packet the system flagged silent).
    pub fn silence(&mut self, frames: usize, out: &mut Vec<i16>) {
        for _ in 0..frames {
            self.feed(0.0, out);
        }
    }

    fn feed(&mut self, x: f32, out: &mut Vec<i16>) {
        let x = match self.filters.as_mut() {
            Some([a, b]) => b.run(a.run(x)),
            None => x,
        };
        while self.pos <= 1.0 {
            let y = self.prev + (x - self.prev) * self.pos as f32;
            out.push((y.clamp(-1.0, 1.0) * 32767.0).round() as i16);
            self.pos += self.step;
        }
        self.pos -= 1.0;
        self.prev = x;
    }
}

/// The loudest sample, 0..1.
pub fn peak(pcm: &[i16]) -> f32 {
    pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, secs: f32, channels: usize) -> Vec<u8> {
        let n = (rate as f32 * secs) as usize;
        let mut out = Vec::new();
        for i in 0..n {
            let v = (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin() * 0.5;
            for _ in 0..channels {
                out.extend(v.to_le_bytes());
            }
        }
        out
    }

    /// Crossings of zero going up, per second.
    fn freq(pcm: &[i16]) -> f32 {
        let ups = pcm.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
        ups as f32 / (pcm.len() as f32 / RATE as f32)
    }

    #[test]
    fn downmix_resamples_speech_to_16k_mono() {
        // 48 kHz stereo float (the usual Windows mix format), a 1 kHz tone.
        let mut d = Downmix::new(48_000, 2, SampleFormat::F32);
        let mut out = Vec::new();
        let p = d.push(&sine(48_000, 1000.0, 1.0, 2), &mut out);
        assert!((out.len() as i64 - 16_000).abs() <= 1, "{}", out.len());
        assert!((freq(&out) - 1000.0).abs() < 5.0, "{}", freq(&out));
        assert!((0.45..0.55).contains(&p), "peak {p}");
        // 44.1 kHz 16-bit mono: the length follows the rate.
        let pcm: Vec<u8> = sine(44_100, 500.0, 1.0, 1).chunks(4).flat_map(|b| (((f32::from_le_bytes(b.try_into().unwrap())) * 32767.0) as i16).to_le_bytes()).collect();
        let mut d = Downmix::new(44_100, 1, SampleFormat::I16);
        let mut out = Vec::new();
        d.push(&pcm, &mut out);
        assert!((out.len() as i64 - 16_000).abs() <= 1, "{}", out.len());
        assert!((freq(&out) - 500.0).abs() < 5.0);
    }

    #[test]
    fn downmix_keeps_tones_above_8k_from_folding_back() {
        let mut d = Downmix::new(48_000, 1, SampleFormat::F32);
        let mut out = Vec::new();
        d.push(&sine(48_000, 12_000.0, 1.0, 1), &mut out);
        // Unfiltered, 12 kHz would land at 4 kHz at full strength (0.5).
        assert!(peak(&out[800..]) < 0.05, "{}", peak(&out[800..]));
    }

    #[test]
    fn downmix_gives_the_same_samples_whatever_the_packets() {
        let input = sine(44_100, 700.0, 0.5, 2);
        let mut whole = Vec::new();
        Downmix::new(44_100, 2, SampleFormat::F32).push(&input, &mut whole);
        let mut parts = Vec::new();
        let mut d = Downmix::new(44_100, 2, SampleFormat::F32);
        for chunk in input.chunks(8 * 37) {
            d.push(chunk, &mut parts);
        }
        assert_eq!(whole, parts);
        // 16 kHz mono 16-bit comes through as it is.
        let pcm: Vec<i16> = (0..1000).map(|i| (i * 13 % 2000 - 1000) as i16).collect();
        let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        Downmix::new(16_000, 1, SampleFormat::I16).push(&bytes, &mut out);
        let back: Vec<i16> = out.iter().map(|s| ((*s as f32 / 32767.0) * 32768.0).round() as i16).collect();
        assert!(back.iter().zip(&pcm).all(|(a, b)| (a - b).abs() <= 1));
        // 24-bit samples read right.
        let mut out = Vec::new();
        Downmix::new(16_000, 1, SampleFormat::I24).push(&[0x00, 0x00, 0x40, 0x00, 0x00, 0xC0], &mut out);
        assert_eq!(out, [16384, -16384]);
        let mut out = Vec::new();
        Downmix::new(48_000, 2, SampleFormat::F32).silence(4800, &mut out);
        assert_eq!((out.len(), peak(&out)), (1600, 0.0));
    }

}
