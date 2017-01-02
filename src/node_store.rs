use dht::KNodeTable;
use dht::GenericNodeTable;

use node::{Address, NodeData, Node};

pub struct NodeStore {
    pub table: KNodeTable<Address, NodeData>,
    pub my_address: Address,
}

impl NodeStore {
    pub fn new(my_address: Address) -> NodeStore {
        NodeStore {
            table: KNodeTable::new(my_address.clone()),
            my_address: my_address,
        }
    }

    pub fn update(&mut self, node: &Node) {
        let insert_suceeded = self.table.update(node);
        if !insert_suceeded {
            self.table.pop_oldest();
            assert!(self.table.update(node));
        }
    }

    pub fn find_closest_nodes(&self, address: &Address, count: usize) -> Vec<Node> {
        self.table.find(address, count)
    }
}
