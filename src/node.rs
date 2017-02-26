use std::fmt;
use std::net::Ipv6Addr;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use rustc_serialize::{Encodable, Encoder, Decoder};

use dht::GenericId;
use dht::Node as DhtNode;

pub use fcp_switching::route_packet::NodeData;

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

pub const ADDRESS_BITS: usize = 16*8;

/// Wrapper of `Ipv6Addr` that implements `GenericId`
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let ipv6addr = Ipv6Addr::from(self);
        write!(f, "Address::from(Ipv6Addr::from_str(\"{}\"))", ipv6addr)
    }
}

impl<'a> From<&'a Ipv6Addr> for Address {
    fn from(ipv6addr: &Ipv6Addr) -> Address {
        Address::new(&ipv6addr.octets())
    }
}
impl From<Ipv6Addr> for Address {
    fn from(ipv6addr: Ipv6Addr) -> Address {
        Address::from(&ipv6addr)
    }
}
impl<'a> From<&'a Address> for Ipv6Addr {
    fn from(addr: &Address) -> Ipv6Addr {
        Ipv6Addr::from(addr.bytes())
    }
}
impl From<Address> for Ipv6Addr {
    fn from(addr: Address) -> Ipv6Addr {
        Ipv6Addr::from(&addr)
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


/// Wrapper for `dht::Node` with renaming for the new meaning of
/// its fields (dht's id == cjdns's address; dht's address == cjdns' data)
#[derive(Clone)]
pub struct Node(pub DhtNode<Address, NodeData>); // TODO: public only to the crate

impl Node {
    pub fn new(addr: Address, pk: [u8; PUBLIC_KEY_LENGTH], path: Path, version: u64) -> Node {
        let data = NodeData {
            public_key: pk,
            path: path,
            version: version,
        };
        Node(DhtNode { id: addr, address: data })
    }
    pub fn address(&self) -> &Address {
        &self.0.id
    }
    pub fn public_key(&self) -> &[u8; PUBLIC_KEY_LENGTH] {
        &self.0.address.public_key
    }
    pub fn path(&self) -> &Path {
        &self.0.address.path
    }
    pub fn version(&self) -> u64 {
        self.0.address.version
    }
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Node({:?}, {:?}, {:?}, {:?})", self.address(), self.public_key(), self.path(), self.version())
    }
}

impl Eq for Node {
}
impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.0.address == other.0.address
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Node) -> Ordering {
        self.0.address.cmp(&other.0.address)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Node) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for Node {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.address.hash(state);
    }
}
