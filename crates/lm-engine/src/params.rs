//! SPA pod construction for runtime parameter control.
//!
//! Filter-chain nodes expose every unlinked plugin control port through their
//! `Props` param as `"<plugin>:<port>" <value>` pairs inside the `params`
//! property. Volume/mute live in the same Props object under the standard
//! SPA_PROP keys.

use libspa::pod::serialize::PodSerializer;
use libspa::pod::{Object, Property, PropertyFlags, Value, ValueArray};

fn props_object(properties: Vec<Property>) -> Vec<u8> {
    let obj = Value::Object(Object {
        type_: libspa::sys::SPA_TYPE_OBJECT_Props,
        id: libspa::sys::SPA_PARAM_Props,
        properties,
    });
    let (cursor, _len) = PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &obj)
        .expect("serializing a Props pod into a Vec cannot fail");
    cursor.into_inner()
}

/// Props pod setting filter-chain plugin control ports, e.g.
/// `[("gate:gt", 0.0631), ("comp:cr", 4.0)]`.
///
/// Values are the plugin's native port values — LSP thresholds are *linear
/// gain*, not dB; convert with [`db_to_linear`] first.
pub fn filter_params(pairs: &[(&str, f32)]) -> Vec<u8> {
    let mut fields = Vec::with_capacity(pairs.len() * 2);
    for (key, value) in pairs {
        fields.push(Value::String((*key).to_string()));
        fields.push(Value::Float(*value));
    }
    props_object(vec![Property {
        key: libspa::sys::SPA_PROP_params,
        flags: PropertyFlags::empty(),
        value: Value::Struct(fields),
    }])
}

/// Props pod setting per-channel linear volume (stereo).
pub fn channel_volumes(volume: f32) -> Vec<u8> {
    props_object(vec![Property {
        key: libspa::sys::SPA_PROP_channelVolumes,
        flags: PropertyFlags::empty(),
        value: Value::ValueArray(ValueArray::Float(vec![volume, volume])),
    }])
}

/// Props pod setting mute.
pub fn mute(muted: bool) -> Vec<u8> {
    props_object(vec![Property {
        key: libspa::sys::SPA_PROP_mute,
        flags: PropertyFlags::empty(),
        value: Value::Bool(muted),
    }])
}

/// dB → linear gain (LSP threshold/makeup ports take linear values).
pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Linear gain → dB.
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 { f32::NEG_INFINITY } else { 20.0 * linear.log10() }
}
