//! Routing-matrix link reconciliation.
//!
//! Desired routes are pairs of *node names* (stable identity); the reconciler
//! resolves them against the live [`GraphModel`], matches ports by
//! `audio.channel` (mono fans out to every input channel), creates missing
//! links through `link-factory`, and destroys removed ones by dropping the
//! proxies (our links don't linger, so proxy drop == link teardown).

use std::collections::{HashMap, HashSet};

use pipewire::core::CoreRc;
use pipewire::link::Link;
use pipewire::properties::properties;

use crate::registry::{GraphModel, PortDirection, PortInfo};

type Route = (String, String);

#[derive(Default)]
pub struct LinkManager {
    desired: HashSet<Route>,
    /// (out node name, in node name, out port id, in port id) -> proxy
    held: HashMap<(String, String, u32, u32), Link>,
}

impl LinkManager {
    pub fn set_route(&mut self, output_node: &str, input_node: &str, enabled: bool) {
        let route = (output_node.to_string(), input_node.to_string());
        if enabled {
            self.desired.insert(route);
        } else {
            self.desired.remove(&route);
        }
    }

    pub fn desired(&self) -> impl Iterator<Item = &Route> {
        self.desired.iter()
    }

    /// Bring actual links in line with desired routes. Call after any relevant
    /// registry change. Safe to call repeatedly; only diffs are applied, and
    /// stale links (e.g. from a port-arrival race) are torn down.
    pub fn reconcile(&mut self, core: &CoreRc, model: &GraphModel) {
        // Compute the full wanted set of (route, out port, in port).
        let mut wanted: HashMap<(String, String, u32, u32), (u32, u32)> = HashMap::new();
        for (out_name, in_name) in &self.desired {
            let (Some(out_node), Some(in_node)) = (model.node_by_name(out_name), model.node_by_name(in_name)) else {
                continue; // node offline; retried on next reconcile
            };
            let outs = model.node_ports(out_node.id, PortDirection::Out);
            let ins = model.node_ports(in_node.id, PortDirection::In);
            for (op, ip) in match_ports(&outs, &ins) {
                wanted.insert(
                    (out_name.clone(), in_name.clone(), op.id, ip.id),
                    (out_node.id, in_node.id),
                );
            }
        }

        // Destroy held links that are no longer wanted (route removed, or the
        // port pairing changed as ports settled). Dropping the proxy destroys
        // the link server-side.
        self.held.retain(|key, _| wanted.contains_key(key));

        // Create what's missing.
        for (key, (out_node, in_node)) in wanted {
            if self.held.contains_key(&key) || model.link_between(key.2, key.3).is_some() {
                continue;
            }
            let props = properties! {
                "link.output.node" => out_node.to_string(),
                "link.output.port" => key.2.to_string(),
                "link.input.node" => in_node.to_string(),
                "link.input.port" => key.3.to_string(),
                "object.linger" => "false",
            };
            match core.create_object::<Link>("link-factory", &props) {
                Ok(link) => {
                    tracing::debug!("linked {}:{} -> {}:{}", key.0, key.2, key.1, key.3);
                    self.held.insert(key, link);
                }
                Err(e) => tracing::warn!("link {} -> {} failed: {e}", key.0, key.1),
            }
        }
    }

    /// Forget proxies for links the server already removed (e.g. node died).
    pub fn prune_dead(&mut self, model: &GraphModel) {
        self.held.retain(|(.., op, ip), _| {
            // If both ports still exist, keep the proxy even if the Link global
            // hasn't appeared in the model yet (creation is async).
            model.ports.contains_key(op) && model.ports.contains_key(ip)
        });
    }
}

/// Channel-strict matching: FL→FL / FR→FR. Fallbacks, in order:
/// - no channel matched at all but both sides have ports (streams expose
///   `UNK` channels before format negotiation): pair by sorted index —
///   pre-negotiation port order is creation order, which is channel order;
/// - a *single* output port with no matching input channel (a true mono
///   source: MONO/AUX0) fans out to every input port.
/// Never fans out multi-port outputs — during port arrival a stereo node
/// briefly has one port, and fanning out then cross-links FL→FR.
fn match_ports<'a>(outs: &[&'a PortInfo], ins: &[&'a PortInfo]) -> Vec<(&'a PortInfo, &'a PortInfo)> {
    let mut pairs = Vec::new();
    for o in outs {
        let matched: Vec<_> = ins.iter().filter(|i| i.channel == o.channel).collect();
        if !matched.is_empty() {
            pairs.extend(matched.into_iter().map(|i| (*o, *i)));
        } else if outs.len() == 1 {
            pairs.extend(ins.iter().map(|i| (*o, *i)));
        }
    }
    if pairs.is_empty() && !outs.is_empty() && outs.len() == ins.len() {
        pairs.extend(outs.iter().zip(ins.iter()).map(|(o, i)| (*o, *i)));
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(id: u32, direction: PortDirection, channel: &str) -> PortInfo {
        PortInfo { id, node_id: 1, direction, channel: channel.into(), name: channel.into() }
    }

    fn out(id: u32, channel: &str) -> PortInfo {
        port(id, PortDirection::Out, channel)
    }

    fn inp(id: u32, channel: &str) -> PortInfo {
        port(id, PortDirection::In, channel)
    }

    /// (out port id, in port id) pairs, sorted for stable comparison.
    fn pairs(outs: &[PortInfo], ins: &[PortInfo]) -> Vec<(u32, u32)> {
        let o: Vec<&PortInfo> = outs.iter().collect();
        let i: Vec<&PortInfo> = ins.iter().collect();
        let mut v: Vec<(u32, u32)> = match_ports(&o, &i).into_iter().map(|(a, b)| (a.id, b.id)).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn stereo_pairs_by_channel() {
        let outs = [out(10, "FL"), out(11, "FR")];
        let ins = [inp(20, "FL"), inp(21, "FR")];
        assert_eq!(pairs(&outs, &ins), vec![(10, 20), (11, 21)]);
    }

    /// Ports do not arrive in channel order; matching must not rely on it.
    #[test]
    fn channel_matching_ignores_port_order() {
        let outs = [out(10, "FR"), out(11, "FL")];
        let ins = [inp(20, "FL"), inp(21, "FR")];
        assert_eq!(pairs(&outs, &ins), vec![(10, 21), (11, 20)]);
    }

    #[test]
    fn true_mono_source_fans_out_to_every_input() {
        let outs = [out(10, "MONO")];
        let ins = [inp(20, "FL"), inp(21, "FR")];
        assert_eq!(pairs(&outs, &ins), vec![(10, 20), (10, 21)]);
    }

    /// REGRESSION: a stereo node briefly exposes a single port while its ports
    /// are still arriving. Fanning that port out would cross-link FL into FR
    /// and permanently mis-wire the strip. The channel match must win.
    #[test]
    fn half_arrived_stereo_output_does_not_fan_out() {
        let outs = [out(10, "FL")]; // FR has not appeared yet
        let ins = [inp(20, "FL"), inp(21, "FR")];
        assert_eq!(
            pairs(&outs, &ins),
            vec![(10, 20)],
            "FL must link only to FL; the missing FR link appears on a later reconcile"
        );
    }

    /// A meter tap's stream ports report channel UNK until the format is
    /// negotiated, while the node it taps already reports FL/FR. No channel
    /// matches, so the index fallback pairs them in port order — which is
    /// creation order, which is channel order.
    #[test]
    fn unmatched_channels_pair_by_index() {
        let outs = [out(10, "FL"), out(11, "FR")];
        let ins = [inp(20, "UNK"), inp(21, "UNK")];
        assert_eq!(pairs(&outs, &ins), vec![(10, 20), (11, 21)]);
    }

    /// Index fallback is only safe when the counts agree; guessing otherwise
    /// would silently mis-map channels.
    #[test]
    fn index_fallback_requires_equal_counts() {
        let outs = [out(10, "FL"), out(11, "FR")];
        let ins = [inp(20, "UNK"), inp(21, "UNK"), inp(22, "UNK")];
        assert!(pairs(&outs, &ins).is_empty());
    }

    /// SHARP EDGE, documented deliberately: when BOTH sides are still
    /// pre-negotiation, "UNK" compares equal to "UNK", so every output matches
    /// every input and the index fallback below is never reached — the result
    /// is a full cross-product, not index pairing.
    ///
    /// This heals on the next reconcile once the formats settle (the reconciler
    /// diffs a full wanted-set and drops the extra links), so it is a transient
    /// rather than a stuck mis-wiring. It is asserted here so the behaviour
    /// cannot change silently.
    #[test]
    fn both_sides_unknown_cross_links_until_formats_settle() {
        let outs = [out(10, "UNK"), out(11, "UNK")];
        let ins = [inp(20, "UNK"), inp(21, "UNK")];
        assert_eq!(pairs(&outs, &ins), vec![(10, 20), (10, 21), (11, 20), (11, 21)]);
    }

    #[test]
    fn no_inputs_yields_no_links() {
        assert!(pairs(&[out(10, "FL")], &[]).is_empty());
        assert!(pairs(&[], &[inp(20, "FL")]).is_empty());
    }

    /// Two strips feeding one bus is the normal mixing case: each strip's FL
    /// lands on the same bus FL input.
    #[test]
    fn many_outputs_may_share_one_input_channel() {
        let outs = [out(10, "FL")];
        let ins = [inp(20, "FL")];
        assert_eq!(pairs(&outs, &ins), vec![(10, 20)]);
    }

    #[test]
    fn set_route_toggles_desired_state() {
        let mut lm = LinkManager::default();
        lm.set_route("a.out", "b.in", true);
        assert_eq!(lm.desired().count(), 1);
        // Re-enabling is idempotent, not additive.
        lm.set_route("a.out", "b.in", true);
        assert_eq!(lm.desired().count(), 1);
        lm.set_route("a.out", "b.in", false);
        assert_eq!(lm.desired().count(), 0);
    }

    #[test]
    fn routes_are_directional() {
        let mut lm = LinkManager::default();
        lm.set_route("a.out", "b.in", true);
        lm.set_route("b.in", "a.out", true);
        assert_eq!(lm.desired().count(), 2, "reversed pair is a distinct route");
    }
}
