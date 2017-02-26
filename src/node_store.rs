use dht::KNodeTable;
use dht::GenericNodeTable;

use node::{Address, NodeData, Node, ADDRESS_BITS};

/// Returns by a request to find a node's path and public key.
#[derive(Clone, Debug, Eq, PartialEq)]
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
        let bucket_size = 32;
        let max_distance = ADDRESS_BITS;
        NodeStore {
            table: KNodeTable::new_with_details(my_address.clone(), bucket_size, max_distance),
            my_address: my_address,
        }
    }

    /// Inserts a node in the NodeStore, poping nodes from full
    /// buckets if necessary.
    pub fn update(&mut self, node: &Node) {
        let insert_suceeded = self.table.update(&node.0);
        if !insert_suceeded {
            self.table.pop_oldest();
            assert!(self.table.update(&node.0));
        }
    }

    /// Retrurns an ordered vector of nodes, which are the closest to the
    /// target address this NodeStore knows about.
    pub fn find_closest_nodes(&self, target: &Address, count: usize) -> Vec<Node> {
        let dhtnodes = self.table.find(target, count);
        let mut nodes = Vec::with_capacity(dhtnodes.len());
        for dhtnode in dhtnodes {
            nodes.push(Node(dhtnode));
        }
        nodes
    }

    /// Tries to get a node. On failure, returns `nb_closest` nodes (or all
    /// nodes in the store, if `nb_closest` is too high) that should be
    /// queried about the searched node.
    pub fn get_node(&self, target: &Address, nb_closest: usize) -> GetNodeResult {
        let closest_nodes = self.find_closest_nodes(target, nb_closest);
        match closest_nodes.clone().get(0) { // TODO: do not clone
            Some(closest_node) => {
                if closest_node.address() == target {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;
    use std::str::FromStr;
    use node::Address;
    use node::Node;

    #[test]
    fn test_get_one_node() {
        let mut ns = NodeStore::new(Address::from(Ipv6Addr::from_str("fc8f:a188:1b5:4de9:b0cb:5729:23a1:60f9").unwrap()));
        let node = Node::new(Address::from(Ipv6Addr::from_str("fc7c:8316:ec7d:1308:d3c2:6db7:5ad9:6ebc").unwrap()), [14, 212, 108, 34, 167, 28, 34, 202, 98, 134, 15, 159, 58, 151, 12, 228, 58, 163, 181, 163, 40, 102,  66, 125, 212, 44, 203, 100, 174, 56, 120, 61], [0, 0, 0, 0, 0, 0, 0, 11], 17);
        ns.update(&node);
        let res = ns.get_node(&Address::from(Ipv6Addr::from_str("fcb9:326d:37d5:c57b:7ee5:28b5:7aa5:525").unwrap()), 42);
        assert_eq!(res, GetNodeResult::ClosestNodes(vec![node]));
    }
}
