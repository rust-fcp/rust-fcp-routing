use std::net::Ipv6Addr;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use rustc_serialize::{Encodable, Encoder, Decoder};

use dht::GenericId;
use dht::Node as DhtNode;

pub const PUBLIC_KEY_LENGTH: usize = 32;

pub type Path = [u8; 8];

/// Rotates an IPv6 address 64 bits, which is a required preprocessing
/// for computing the XOR metric.
/// See https://github.com/cjdelisle/cjdns/blob/cjdns-v18/doc/Whitepaper.md#the-router
fn rotate_64(i: &[u8; 16]) -> [u8; 16] {
    [
        i[ 8], i[ 9], i[10], i[11], i[12], i[13], i[14], i[15],
        i[ 0], i[ 1], i[ 2], i[ 3], i[ 4], i[ 5], i[ 6], i[ 7],
    ]
}

/// Wrapper of `Ipv6Addr` that implements `GenericId`
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Address {
    bytes: [u8; 16],
}

impl Address {
    pub fn new(bytes: &[u8; 16]) -> Address {
        Address { bytes: rotate_64(bytes) }
    }
    pub fn bytes(&self) -> [u8; 16] {
        rotate_64(&self.bytes)
    }
    pub fn from_ipv6addr(ipv6addr: &Ipv6Addr) -> Address {
        Address::new(&ipv6addr.octets())
    }
    pub fn to_ipv6addr(&self) -> Ipv6Addr {
        Ipv6Addr::from(self.bytes())
    }
}

impl GenericId for Address {
    fn bitxor(&self, other: &Self) -> Self {
        let vec = self.bytes.to_vec().bitxor(&other.bytes.to_vec());
        assert_eq!(vec.len(), 16);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&vec);
        Address { bytes: bytes }
    }
    fn is_zero(&self) -> bool {
        self.bytes.to_vec().is_zero()
    }
    fn bits(&self) -> usize {
        self.bytes.to_vec().bits()
    }
    fn gen(bit_size: usize) -> Self {
        let vec = Vec::<u8>::gen(bit_size);
        assert_eq!(vec.len(), 16);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&vec);
        Address { bytes: bytes }
    }
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {
        self.bytes.encode(s)
    }
    fn decode<D: Decoder>(d: &mut D) -> Result<Self, D::Error> {
        let vec = try!(Vec::<u8>::decode(d));
        assert_eq!(vec.len(), 16);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&vec);
        Ok(Address { bytes: bytes })
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
