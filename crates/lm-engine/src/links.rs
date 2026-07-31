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
