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

        // F32 (interleaved), native rate/channels.
        let mut audio_info = libspa::param::audio::AudioInfoRaw::new();
        audio_info.set_format(libspa::param::audio::AudioFormat::F32LE);
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
fn bytemuck_cast(bytes: &[u8]) -> &[f32] {
    let n = bytes.len() / 4;
    // SAFETY: f32 has 4-byte alignment; PipeWire buffers are page-aligned, and
    // we only expose complete f32s.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<f32>(), n) }
}
