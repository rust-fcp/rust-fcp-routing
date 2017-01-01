use std::net::Ipv6Addr;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use rustc_serialize::{Encodable, Encoder, Decoder};

use dht::GenericId;
use dht::Node as DhtNode;

pub const PUBLIC_KEY_LENGTH: usize = 32;

pub type Path = [u8; 8];

/// Wrapper of `Ipv6Addr` that implements `GenericId`
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Address {
    pub octets: [u8; 16],
}

impl Address {
    pub fn from_ipv6addr(ipv6addr: &Ipv6Addr) -> Address {
        Address { octets: ipv6addr.octets() }
    }
    pub fn to_ipv6addr(&self) -> Ipv6Addr {
        Ipv6Addr::from(self.octets)
    }
}

impl GenericId for Address {
    fn bitxor(&self, other: &Self) -> Self {
        let vec = self.octets.to_vec().bitxor(&other.octets.to_vec());
        assert_eq!(vec.len(), 16);
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&vec);
        Address { octets: octets }
    }
    fn is_zero(&self) -> bool {
        self.octets.to_vec().is_zero()
    }
    fn bits(&self) -> usize {
        self.octets.to_vec().bits()
    }
    fn gen(bit_size: usize) -> Self {
        let vec = Vec::<u8>::gen(bit_size);
        assert_eq!(vec.len(), 16);
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&vec);
        Address { octets: octets }
    }
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        self.octets.encode(s)
    }
    fn decode<D: Decoder>(d: &mut D) -> Result<Self, D::Error> {
        let vec = try!(Vec::<u8>::decode(d));
        assert_eq!(vec.len(), 16);
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&vec);
        Ok(Address { octets: octets })
    }
}

#[derive(Debug, Clone, Eq, PartialOrd)]
pub struct NodeData {
    pub public_key: [u8; PUBLIC_KEY_LENGTH],
    pub path: Path,
    pub version: u64,
}

impl PartialEq for NodeData {
    fn eq(&self, other: &NodeData) -> bool {
        self.public_key == other.public_key
    }
}

impl Ord for NodeData {
    fn cmp(&self, other: &NodeData) -> Ordering {
        self.public_key.cmp(&other.public_key)
    }
}

impl Hash for NodeData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.public_key.hash(state);
    }
}

type Node = DhtNode<Address, NodeData>;
