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

#[cfg(test)]
mod tests {
    use super::*;
    use libspa::pod::deserialize::PodDeserializer;

    /// Decode a Props pod back into (id, properties) so tests assert on what
    /// the server would actually receive.
    fn decode(bytes: &[u8]) -> Object {
        let (_rest, value) =
            PodDeserializer::deserialize_any_from(bytes).expect("pod must deserialize");
        match value {
            Value::Object(o) => o,
            other => panic!("expected an Object pod, got {other:?}"),
        }
    }

    fn only_property(bytes: &[u8], key: u32) -> Value {
        let obj = decode(bytes);
        assert_eq!(obj.type_, libspa::sys::SPA_TYPE_OBJECT_Props);
        assert_eq!(obj.id, libspa::sys::SPA_PARAM_Props);
        let prop = obj
            .properties
            .iter()
            .find(|p| p.key == key)
            .unwrap_or_else(|| panic!("no property with key {key}"));
        prop.value.clone()
    }

    #[test]
    fn db_linear_roundtrip() {
        for db in [-60.0, -18.0, -6.0, -1.0, 0.0, 6.0, 12.0] {
            let back = linear_to_db(db_to_linear(db));
            assert!((back - db).abs() < 1e-3, "{db} dB roundtripped to {back}");
        }
    }

    #[test]
    fn db_to_linear_known_values() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
        // -6 dB is very nearly half amplitude; -20 dB is exactly a tenth.
        assert!((db_to_linear(-6.0) - 0.501_187).abs() < 1e-5);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn linear_to_db_handles_silence() {
        assert_eq!(linear_to_db(0.0), f32::NEG_INFINITY);
        assert_eq!(linear_to_db(-1.0), f32::NEG_INFINITY);
    }

    /// The filter-chain contract: `params` is a Struct of alternating
    /// String/Float. Getting this shape wrong silently no-ops every DSP knob.
    #[test]
    fn filter_params_alternates_string_and_float() {
        let bytes = filter_params(&[("gate:gt", 0.063), ("comp:cr", 4.0)]);
        let Value::Struct(fields) = only_property(&bytes, libspa::sys::SPA_PROP_params) else {
            panic!("params must be a Struct");
        };
        assert_eq!(fields.len(), 4);
        assert!(matches!(&fields[0], Value::String(s) if s == "gate:gt"));
        assert!(matches!(fields[1], Value::Float(v) if (v - 0.063).abs() < 1e-6));
        assert!(matches!(&fields[2], Value::String(s) if s == "comp:cr"));
        assert!(matches!(fields[3], Value::Float(v) if (v - 4.0).abs() < 1e-6));
    }

    #[test]
    fn filter_params_accepts_an_empty_set() {
        let bytes = filter_params(&[]);
        let Value::Struct(fields) = only_property(&bytes, libspa::sys::SPA_PROP_params) else {
            panic!("params must be a Struct");
        };
        assert!(fields.is_empty());
    }

    #[test]
    fn channel_volumes_is_stereo_and_linear() {
        let bytes = channel_volumes(db_to_linear(-6.0));
        let Value::ValueArray(ValueArray::Float(v)) =
            only_property(&bytes, libspa::sys::SPA_PROP_channelVolumes)
        else {
            panic!("channelVolumes must be a Float array");
        };
        assert_eq!(v.len(), 2, "both channels must be set");
        assert_eq!(v[0], v[1]);
        assert!((v[0] - 0.501_187).abs() < 1e-5, "must be linear gain, not dB");
    }

    #[test]
    fn mute_pod_carries_a_bool() {
        assert!(matches!(
            only_property(&mute(true), libspa::sys::SPA_PROP_mute),
            Value::Bool(true)
        ));
        assert!(matches!(
            only_property(&mute(false), libspa::sys::SPA_PROP_mute),
            Value::Bool(false)
        ));
    }
}
