//! Live graph model fed by registry events.
//!
//! Tracks nodes, ports, and links by transient global id, with stable identity
//! (`node.name`, `object.serial`) kept alongside so profiles never depend on
//! recycled ids.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInfo {
    pub id: u32,
    pub serial: u64,
    pub name: String,
    pub description: String,
    pub media_class: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    In,
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    pub id: u32,
    pub node_id: u32,
    pub direction: PortDirection,
    /// `audio.channel` — "FL", "FR", "MONO", …
    pub channel: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkInfo {
    pub id: u32,
    pub output_node: u32,
    pub output_port: u32,
    pub input_node: u32,
    pub input_port: u32,
}

/// Snapshot of the parts of the PipeWire graph we care about.
#[derive(Debug, Default)]
pub struct GraphModel {
    pub nodes: HashMap<u32, NodeInfo>,
    pub ports: HashMap<u32, PortInfo>,
    pub links: HashMap<u32, LinkInfo>,
}

impl GraphModel {
    pub fn node_by_name(&self, name: &str) -> Option<&NodeInfo> {
        self.nodes.values().find(|n| n.name == name)
    }

    /// Ports of a node in a given direction, keyed by channel.
    pub fn node_ports(&self, node_id: u32, direction: PortDirection) -> Vec<&PortInfo> {
        let mut v: Vec<&PortInfo> = self
            .ports
            .values()
            .filter(|p| p.node_id == node_id && p.direction == direction)
            .collect();
        v.sort_by_key(|p| p.id);
        v
    }

    /// Does a link between these two ports exist?
    pub fn link_between(&self, output_port: u32, input_port: u32) -> Option<&LinkInfo> {
        self.links
            .values()
            .find(|l| l.output_port == output_port && l.input_port == input_port)
    }

    pub fn remove_global(&mut self, id: u32) {
        self.nodes.remove(&id);
        self.ports.remove(&id);
        self.links.remove(&id);
    }
}

/// Parse a registry `global` event's props into our model types.
pub mod parse {
    use super::*;
    use libspa::utils::dict::DictRef;

    pub fn node(id: u32, props: &DictRef) -> Option<NodeInfo> {
        let name = props.get("node.name")?.to_string();
        Some(NodeInfo {
            id,
            serial: props.get("object.serial").and_then(|s| s.parse().ok()).unwrap_or(0),
            description: props.get("node.description").unwrap_or(&name).to_string(),
            media_class: props.get("media.class").unwrap_or("").to_string(),
            name,
        })
    }

    pub fn port(id: u32, props: &DictRef) -> Option<PortInfo> {
        let node_id: u32 = props.get("node.id")?.parse().ok()?;
        let direction = match props.get("port.direction")? {
            "in" => PortDirection::In,
            "out" => PortDirection::Out,
            _ => return None,
        };
        Some(PortInfo {
            id,
            node_id,
            direction,
            channel: props.get("audio.channel").unwrap_or("UNK").to_string(),
            name: props.get("port.name").unwrap_or("").to_string(),
        })
    }

    pub fn link(id: u32, props: &DictRef) -> Option<LinkInfo> {
        Some(LinkInfo {
            id,
            output_node: props.get("link.output.node")?.parse().ok()?,
            output_port: props.get("link.output.port")?.parse().ok()?,
            input_node: props.get("link.input.node")?.parse().ok()?,
            input_port: props.get("link.input.port")?.parse().ok()?,
        })
    }
}
