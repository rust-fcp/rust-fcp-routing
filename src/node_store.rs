use dht::KNodeTable;
use dht::GenericNodeTable;

use node::{Address, NodeData, Node};

/// Returns by a request to find a node's path and public key.
#[derive(Clone, Debug)]
pub enum GetNodeResult {
    /// The exact node, if it was found.
    FoundNode(Node),
    /// Nodes close to the searched node. They should be queried about
    /// the searched node.
    ClosestNodes(Vec<Node>),
    /// This table knows of no node at all. It should be bootstrapped
    /// using an external way (ie. find some peers).
    Nothing,
}

pub struct NodeStore {
    pub table: KNodeTable<Address, NodeData>,
    pub my_address: Address,
}

impl NodeStore {
    /// Creates a new empty NodeStore.
    pub fn new(my_address: Address) -> NodeStore {
        NodeStore {
            table: KNodeTable::new(my_address.clone()),
            my_address: my_address,
        }
    }

    /// Inserts a node in the NodeStore, poping nodes from full
    /// buckets if necessary.
    pub fn update(&mut self, node: &Node) {
        let insert_suceeded = self.table.update(node);
        if !insert_suceeded {
            self.table.pop_oldest();
            assert!(self.table.update(node));
        }
    }

    /// Retrurns an ordered vector of nodes, which are the closest to the
    /// target address this NodeStore knows about.
    pub fn find_closest_nodes(&self, target: &Address, count: usize) -> Vec<Node> {
        self.table.find(target, count)
    }

    /// Tries to get a node. On failure, returns `nb_closest` nodes (or all
    /// nodes in the store, if `nb_closest` is too high) that should be
    /// queried about the searched node.
    pub fn get_node(&self, target: &Address, nb_closest: usize) -> GetNodeResult {
        let closest_nodes = self.find_closest_nodes(target, nb_closest);
        match closest_nodes.clone().get(0) { // TODO: do not clone
            Some(closest_node) => {
                if closest_node.id == *target {
                    GetNodeResult::FoundNode(closest_node.clone())
                }
                else {
                    GetNodeResult::ClosestNodes(closest_nodes)
                }
            }
            None => GetNodeResult::Nothing,
        }
    }
}
