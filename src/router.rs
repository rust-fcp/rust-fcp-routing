use fcp_switching::route_packet::{RoutePacket, RoutePacketBuilder};
use fcp_switching::operation::Label;

use node_store::{NodeStore, GetNodeResult};
use node::{Address, Node};

const PROTOCOL_VERSION: i64 = 18;


/// Wrapper of `NodeStore` that reads/writes network packets.
/// TODO: Check paths are valid before inserting them (eg. send a
/// ping and wait for the reply).
pub struct Router {
    node_store: NodeStore,
}

impl Router {
    pub fn new(my_address: Address) -> Router {
        Router {
            node_store: NodeStore::new(my_address),
        }
    }

    /// See `NodeStore::update`.
    pub fn update(&mut self, node: &Node) {
        self.node_store.update(node)
    }

    /// Wrapper for `NodeStore::get_node` that returns RoutePackets that
    /// should be sent in order to fetch the target node.
    pub fn get_node(&self, target: &Address, nb_closest: usize) -> (Option<Node>, Vec<(Label, RoutePacket)>) {
        match self.node_store.get_node(target, nb_closest) {
            GetNodeResult::FoundNode(node) => (Some(node), Vec::new()),
            GetNodeResult::ClosestNodes(nodes) => {
                // Ask each of the closest nodes about the target
                let requests = nodes.iter().map(|node| {
                    let packet = RoutePacketBuilder::new(PROTOCOL_VERSION, Vec::new())
                            .query("fn".to_owned())
                            .target_address(target.bytes().to_vec())
                            .finalize();
                    (node.address.path, packet)
                });
                let requests = requests.collect();
                (None, requests)
            }
            GetNodeResult::Nothing => {
                // TODO: do something
                (None, Vec::new())
            }
        }
    }

    /// Called when a RoutePacket is received from the network.
    /// Optionally returns RoutePackets to send back.
    pub fn on_route_packet(&mut self, label: &Label, packet: &RoutePacket) -> Result<Vec<(Label, RoutePacket)>, ()> {
        Ok(Vec::new())
    }
}