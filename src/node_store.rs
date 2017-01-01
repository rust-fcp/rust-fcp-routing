use dht::KNodeTable;

use node::{Address, NodeData};

pub type NodeStore = KNodeTable<Address, NodeData>;
