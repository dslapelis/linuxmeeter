//! Peak/RMS metering taps.
//!
//! One lightweight F32 capture stream per metered point, connected WITHOUT
//! autoconnect — the [`crate::links::LinkManager`] links each tap to its
//! target node like any other route, so metering needs no session-manager
//! cooperation. The process callback accumulates peak + sum-of-squares into
//! atomics; a 30 Hz timer drains every tap into one batched frame.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use libspa::pod::Pod;
use pipewire::core::CoreRc;
use pipewire::properties::properties;
use pipewire::stream::{StreamFlags, StreamListener, StreamRc};

const SILENT_DB: f32 = -90.0;

/// Lock-free accumulator shared between the process callback and the drain timer.
#[derive(Default)]
pub struct MeterAccum {
    /// Bit-cast f32 peak per channel (reset to 0.0 on drain).
    peak: [AtomicU32; 2],
    /// Sum of squares per channel, f64 bits (f64 loses no precision at meter timescales).
    sum_sq: [AtomicU64; 2],
    samples: [AtomicU64; 2],
}

impl MeterAccum {
    fn accumulate(&self, ch: usize, peak: f32, sum_sq: f64, n: u64) {
        // CAS-max on the peak.
        let _ = self.peak[ch].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            (peak > f32::from_bits(cur)).then(|| peak.to_bits())
        });
        let _ = self.sum_sq[ch].fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            Some((f64::from_bits(cur) + sum_sq).to_bits())
        });
        self.samples[ch].fetch_add(n, Ordering::Relaxed);
    }

    /// Read-and-reset; returns [peak_l, peak_r, rms_l, rms_r] in dBFS.
    pub fn drain(&self) -> [f32; 4] {
        let mut out = [SILENT_DB; 4];
        for ch in 0..2 {
            let peak = f32::from_bits(self.peak[ch].swap(0, Ordering::Relaxed));
            let sum_sq = f64::from_bits(self.sum_sq[ch].swap(0, Ordering::Relaxed));
            let n = self.samples[ch].swap(0, Ordering::Relaxed);
            out[ch] = amp_to_db(peak);
            out[2 + ch] = if n > 0 { amp_to_db((sum_sq / n as f64).sqrt() as f32) } else { SILENT_DB };
        }
        out
    }
}

fn amp_to_db(a: f32) -> f32 {
    if a <= 1e-4_f32 {
        SILENT_DB
    } else {
        (20.0 * a.log10()).max(SILENT_DB)
    }
}

/// A capture tap on one node's output. Link it with
/// `LinkManager::set_route(<target node>, "<tap node name>", true)`.
pub struct MeterTap {
    pub node_name: String,
    pub accum: Arc<MeterAccum>,
    _stream: StreamRc,
    _listener: StreamListener<u32>,
}

impl MeterTap {
    /// `key` names the tap node `lm.meter.<key>`; link its input to any output
    /// ports via the routing matrix.
    pub fn new(core: CoreRc, key: &str) -> Result<Self, pipewire::Error> {
        let node_name = format!("lm.meter.{key}");
        let props = properties! {
            *pipewire::keys::MEDIA_TYPE => "Audio",
            *pipewire::keys::MEDIA_CATEGORY => "Capture",
            *pipewire::keys::NODE_NAME => node_name.as_str(),
            *pipewire::keys::NODE_AUTOCONNECT => "false",
            *pipewire::keys::NODE_PASSIVE => "true",
            "node.virtual" => "true",
        };
        let stream = StreamRc::new(core, &node_name, props)?;
        let accum = Arc::new(MeterAccum::default());

        let accum_cb = accum.clone();
        // user data = channel count learned from the negotiated format.
        let listener = stream
            .add_local_listener_with_user_data::<u32>(2)
            .param_changed(|_, channels, id, param| {
                let Some(param) = param else { return };
                if id != libspa::param::ParamType::Format.as_raw() {
                    return;
                }
                let mut info = libspa::param::audio::AudioInfoRaw::new();
                if info.parse(param).is_ok() && info.channels() > 0 {
                    *channels = info.channels();
                }
            })
            .process(move |stream, channels| {
                while let Some(mut buffer) = stream.dequeue_buffer() {
                    let n_ch = (*channels).max(1) as usize;
                    let datas = buffer.datas_mut();
                    let Some(data) = datas.first_mut() else { continue };
                    let n_bytes = data.chunk().size() as usize;
                    let Some(samples) = data.data() else { continue };
                    let floats: &[f32] = bytemuck_cast(&samples[..n_bytes.min(samples.len())]);
                    for ch in 0..n_ch.min(2) {
                        let mut peak = 0f32;
                        let mut sum_sq = 0f64;
                        let mut n = 0u64;
                        let mut i = ch;
                        while i < floats.len() {
                            let s = floats[i];
                            peak = peak.max(s.abs());
                            sum_sq += (s as f64) * (s as f64);
                            n += 1;
                            i += n_ch;
                        }
                        accum_cb.accumulate(ch, peak, sum_sq, n);
                    }
                }
            })
            .register()?;

        // F32 (interleaved) stereo at whatever rate the graph runs.
        //
        // The channel count is declared rather than left open: ports are created
        // from the requested format, and a tap with no declared channels has no
        // ports until something negotiates one for it — which, with autoconnect
        // off, nothing ever does. Declaring stereo is what keeps metering
        // independent of the session manager. `MeterAccum` tracks two channels
        // regardless, so this asks for exactly what it can use.
        let mut audio_info = libspa::param::audio::AudioInfoRaw::new();
        audio_info.set_format(libspa::param::audio::AudioFormat::F32LE);
        audio_info.set_channels(2);
        let mut position = [0u32; libspa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
        position[0] = libspa::sys::SPA_AUDIO_CHANNEL_FL;
        position[1] = libspa::sys::SPA_AUDIO_CHANNEL_FR;
        audio_info.set_position(position);
        let obj = libspa::pod::Object {
            type_: libspa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: libspa::param::ParamType::EnumFormat.as_raw(),
            properties: audio_info.into(),
        };
        let bytes = libspa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &libspa::pod::Value::Object(obj),
        )
        .expect("serialize format pod")
        .0
        .into_inner();
        let mut params = [Pod::from_bytes(&bytes).expect("valid format pod")];

        stream.connect(
            libspa::utils::Direction::Input,
            None,
            StreamFlags::MAP_BUFFERS,
            &mut params,
        )?;

        Ok(Self { node_name, accum, _stream: stream, _listener: listener })
    }
}

/// &[u8] -> &[f32] without a bytemuck dependency; truncates any ragged tail.
///
/// PipeWire's mapped buffers are page-aligned, so the alignment check below is
/// never taken in the audio path — but it is one predictable branch, and
/// without it a misaligned caller would be instant undefined behaviour rather
/// than a silent meter.
fn bytemuck_cast(bytes: &[u8]) -> &[f32] {
    // No panic and no logging: this runs in the realtime process callback,
    // where a dropped meter frame is the only acceptable failure.
    if bytes.as_ptr().align_offset(std::mem::align_of::<f32>()) != 0 {
        return &[];
    }
    let n = bytes.len() / 4;
    // SAFETY: alignment is checked above and we only expose complete f32s.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<f32>(), n) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Accumulate one channel's worth of samples the way the process callback
    /// does, so the tests exercise the real arithmetic.
    fn feed(accum: &MeterAccum, ch: usize, samples: &[f32]) {
        let mut peak = 0f32;
        let mut sum_sq = 0f64;
        for s in samples {
            peak = peak.max(s.abs());
            sum_sq += (*s as f64) * (*s as f64);
        }
        accum.accumulate(ch, peak, sum_sq, samples.len() as u64);
    }

    fn sine(amplitude: f32, cycles: usize, per_cycle: usize) -> Vec<f32> {
        (0..cycles * per_cycle)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * i as f32 / per_cycle as f32).sin())
            .collect()
    }

    #[test]
    fn silence_reads_as_the_floor() {
        let accum = MeterAccum::default();
        assert_eq!(accum.drain(), [SILENT_DB; 4]);
    }

    #[test]
    fn full_scale_peak_is_zero_dbfs() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &[1.0, -1.0, 0.5]);
        let [peak_l, _, _, _] = accum.drain();
        assert!(peak_l.abs() < 0.01, "expected ~0 dBFS, got {peak_l}");
    }

    /// A sine's RMS is its amplitude / sqrt(2) — 3.01 dB below its peak. This
    /// is the number the on-screen meter shows, so it has to be right.
    #[test]
    fn sine_rms_sits_3_db_below_peak() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &sine(1.0, 100, 64));
        let [peak, _, rms, _] = accum.drain();
        assert!((peak - 0.0).abs() < 0.1, "peak {peak}");
        assert!((rms - -3.01).abs() < 0.1, "rms {rms} should be ~-3.01 dBFS");
    }

    #[test]
    fn half_amplitude_reads_6_db_down() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &sine(0.5, 100, 64));
        let [peak, _, rms, _] = accum.drain();
        assert!((peak - -6.02).abs() < 0.1, "peak {peak}");
        assert!((rms - -9.03).abs() < 0.1, "rms {rms}");
    }

    #[test]
    fn channels_are_independent() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &[1.0]);
        feed(&accum, 1, &[0.1]);
        let [peak_l, peak_r, ..] = accum.drain();
        assert!(peak_l.abs() < 0.01, "left {peak_l}");
        assert!((peak_r - -20.0).abs() < 0.1, "right {peak_r}");
    }

    /// The 30 Hz drain must reset, or a single transient would latch the meter
    /// at its peak forever.
    #[test]
    fn drain_resets_the_accumulator() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &[1.0]);
        assert!(accum.drain()[0].abs() < 0.01);
        assert_eq!(accum.drain(), [SILENT_DB; 4], "second drain must be silent");
    }

    /// Several process callbacks land between drains; peak is a max over all
    /// of them, not just the last.
    #[test]
    fn peak_is_the_max_across_callbacks() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &[0.1]);
        feed(&accum, 0, &[1.0]);
        feed(&accum, 0, &[0.1]);
        assert!(accum.drain()[0].abs() < 0.01);
    }

    /// RMS must be energy-weighted across callbacks, not an average of
    /// averages: a loud burst followed by silence is quieter overall.
    #[test]
    fn rms_accumulates_energy_across_callbacks() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &vec![1.0; 100]); // full scale
        feed(&accum, 0, &vec![0.0; 300]); // then silence
        // mean square = (100 * 1.0) / 400 = 0.25 -> rms 0.5 -> -6.02 dBFS
        let rms = accum.drain()[2];
        assert!((rms - -6.02).abs() < 0.1, "rms {rms}");
    }

    #[test]
    fn very_quiet_signals_clamp_to_the_floor() {
        let accum = MeterAccum::default();
        feed(&accum, 0, &[1e-6]);
        let [peak, _, rms, _] = accum.drain();
        assert_eq!(peak, SILENT_DB);
        assert_eq!(rms, SILENT_DB);
    }

    #[test]
    fn ragged_byte_tails_are_truncated_not_misread() {
        // Build the bytes from f32s so the buffer is genuinely f32-aligned,
        // the way PipeWire's mapped buffers are.
        let backing = [1.0f32, 2.0, 3.0];
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(backing.as_ptr().cast::<u8>(), std::mem::size_of_val(&backing))
        };
        assert_eq!(bytemuck_cast(&bytes[..9]).len(), 2, "a ragged tail is dropped, not misread");
        assert_eq!(bytemuck_cast(bytes), [1.0, 2.0, 3.0]);
        assert!(bytemuck_cast(&bytes[..3]).is_empty());
    }

    /// A misaligned buffer must degrade to an empty frame. Reading f32s
    /// straight off an unaligned pointer is undefined behaviour, and it aborts
    /// the process under Rust's runtime checks.
    #[test]
    fn misaligned_buffer_yields_no_samples_instead_of_ub() {
        let backing = [0.0f32; 4];
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(backing.as_ptr().cast::<u8>(), std::mem::size_of_val(&backing))
        };
        assert!(bytemuck_cast(&bytes[1..]).is_empty());
    }
}
