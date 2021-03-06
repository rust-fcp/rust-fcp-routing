extern crate hex;
extern crate rand;
extern crate byteorder;
extern crate fcp_cryptoauth;
extern crate fcp_switching;
extern crate fcp_routing;

use byteorder::BigEndian;
use byteorder::ByteOrder;

use std::net::{UdpSocket, SocketAddr, IpAddr, Ipv6Addr};
use std::iter::FromIterator;
use std::collections::HashMap;
use std::str::FromStr;

use fcp_cryptoauth::wrapper::*;
use fcp_cryptoauth::keys::ToBase32;

use fcp_switching::switch_packet::SwitchPacket;
use fcp_switching::switch_packet::Payload as SwitchPayload;
use fcp_switching::operation::{RoutingDecision, reverse_label};
use fcp_switching::control::ControlPacket;
use fcp_switching::route_packet::{RoutePacket, RoutePacketBuilder, NodeData};
use fcp_switching::data_packet::DataPacket;
use fcp_switching::data_packet::Payload as DataPayload;
use fcp_switching::encoding_scheme::{EncodingScheme, EncodingSchemeForm};

use fcp_routing::node::{Address, Node};
use fcp_routing::router::Router;

use hex::ToHex;
use rand::Rng;

/// Used to represent a connection to a *direct peer* of this switch.
///
struct Interface {
    /// Used for routing -- it is the Director.
    id: u8,
    /// A point-to-point (aka outer) CryptoAuth session.
    ca_session: Wrapper<String>,
    /// The address where to send the UDP packets to.
    addr: SocketAddr,
}

/// Creates a reply switch packet to an other switch packet.
/// The content of the reply is given as a byte array (returned CryptoAuth's
/// `wrap_messages`).
fn make_reply<PeerId: Clone>(replied_to_packet: &SwitchPacket, reply_content: Vec<u8>, inner_conn: &Wrapper<PeerId>) -> SwitchPacket {
    let first_four_bytes = BigEndian::read_u32(&reply_content[0..4]);
    if first_four_bytes < 4 {
        // If it is a CryptoAuth handshake packet, send it as is.
        SwitchPacket::new_reply(&replied_to_packet, SwitchPayload::CryptoAuthHandshake(reply_content))
    }
    else if first_four_bytes == 0xffffffff {
        // Control packet
        unimplemented!()
    }
    else {
        // Otherwise, it is a CryptoAuth data packet. We have to prepend
        // the session handle to the reply.
        // This handle is used by the peer to know this packet is coming
        // from us.
        let peer_handle = inner_conn.peer_session_handle().unwrap();
        SwitchPacket::new_reply(&replied_to_packet, SwitchPayload::CryptoAuthData(peer_handle, reply_content))
    }
}

/// Main data structure of the switch.
struct Pinger {
    /// The socket used for receiving and sending UDP packets to peers.
    sock: UdpSocket,
    /// Peers
    interfaces: Vec<Interface>,
    /// My public key, both for outer and inner CryptoAuth sessions.
    my_pk: PublicKey,
    /// My public key, both for outer and inner CryptoAuth sessions.
    my_sk: SecretKey,
    /// CryptoAuth sessions used to talk to switches/routers. Their packets
    /// themselves are wrapped in SwitchPackets, which are wrapped in the
    /// outer CryptoAuth sessions.
    inner_conns: HashMap<u32, ([u8; 8], Wrapper<String>)>,
    /// Credentials of peers which are allowed to connect to us.
    allowed_peers: HashMap<Credentials, String>,

    ping_targets: Vec<Address>,
    ping_nodes: Vec<Node>,
    address_to_handle: HashMap<Address, u32>,

    router: Router,
}

impl Pinger {
    /// Instanciates a switch.
    fn new(sock: UdpSocket, interfaces: Vec<Interface>, my_pk: PublicKey, my_sk: SecretKey, allowed_peers: HashMap<Credentials, String>, ping_targets: Vec<Address>) -> Pinger {
        Pinger {
            sock: sock,
            interfaces: interfaces,
            inner_conns: HashMap::new(),
            my_pk: my_pk,
            my_sk: my_sk,
            allowed_peers: allowed_peers,
            ping_targets: ping_targets,
            ping_nodes: Vec::new(),
            address_to_handle: HashMap::new(),
            router: Router::new(Address::from(&publickey_to_ipv6addr(&my_pk)))
            }
    }

    /// Takes a 3-bit interface id, and reverse its bits.
    /// Used to compute reverse paths.
    fn reverse_iface_id(&self, iface_id: u8) -> u8 {
        match iface_id {
            0b000 => 0b000,
            0b001 => 0b100,
            0b010 => 0b010,
            0b011 => 0b110,
            0b100 => 0b001,
            0b101 => 0b101,
            0b110 => 0b011,
            0b111 => 0b111,
            _ => panic!("Iface id greater than 0b111"),
        }
    }

    /// Sometimes (random) sends a switch as a reply to the packet.
    fn random_send_switch_ping(&mut self, switch_packet: &SwitchPacket) {
        if rand::thread_rng().next_u32() > 0xafffffff {
            let ping = ControlPacket::Ping { version: 18, opaque_data: vec![1, 2, 3, 4, 5, 6, 7, 8] };
            let mut packet_response = SwitchPacket::new_reply(&switch_packet, SwitchPayload::Control(ping));
            self.send(&mut packet_response, 0b001);
        }
    }

    /// Send a packet to the appropriate interface.
    fn send(&mut self, packet: &mut SwitchPacket, from_interface: u8) {
        // Logically advance the packet through an interface.
        let routing_decision = packet.switch(3, &(self.reverse_iface_id(from_interface) as u64));
        match routing_decision {
            RoutingDecision::SelfInterface(_) => {
                // Packet is sent to myself
                self.on_self_interface_switch_packet(packet);
            }
            RoutingDecision::Forward(iface_id) => {
                // Packet is sent to a peer.
                let mut sent = false;
                for interface in self.interfaces.iter_mut() {
                    if interface.id as u64 == iface_id {
                        sent = true;
                        // Wrap the packet with the outer CryptoAuth session
                        // of this peer, and send it.
                        for packet in interface.ca_session.wrap_message(&packet.raw) {
                            self.sock.send_to(&packet, interface.addr).unwrap();
                        }
                    }
                }
                if !sent {
                    panic!(format!("Iface {} not found for packet: {:?}", iface_id, packet));
                }
            }
        }
    }

    fn send_message_to_node(&mut self, node: &Node, message: DataPacket) {
        let node_pk = PublicKey::from_slice(node.public_key()).unwrap();
        let addr = publickey_to_ipv6addr(&node_pk).into();
        let handle_opt = self.address_to_handle.get(&addr).map(|h| *h);
        match handle_opt {
            Some(handle) => self.send_message_to_handle(handle, message),
            None => {
                println!("Creating CA session for node {}", Ipv6Addr::from(&addr));
                let credentials = Credentials::None;
                let conn = Wrapper::new_outgoing_connection(
                        self.my_pk.clone(), self.my_sk.clone(),
                        node_pk,
                        credentials, None,
                        format!("outgoing inner {}", Ipv6Addr::from(&addr)), None);
                let handle = self.gen_handle();
                self.address_to_handle.insert(addr.into(), handle);
                self.inner_conns.insert(handle, (node.path().clone(), conn));
                self.send_message_to_handle(handle, message)
            }
        }
    }

    fn send_message_to_handle(&mut self, handle: u32, message: DataPacket) {
        let mut packets = Vec::new();
        {
            let &mut (path, ref mut inner_conn) = self.inner_conns.get_mut(&handle).unwrap();
            println!("Sending inner ca message to handle {} with path {:?}: {}", handle, path, message);
            for packet_response in inner_conn.wrap_message_immediately(&message.raw) {
                let switch_packet = SwitchPacket::new(&path, SwitchPayload::CryptoAuthData(inner_conn.peer_session_handle().unwrap(), packet_response));
                packets.push(switch_packet);
            }
        }
        for mut packet in packets {
            self.send(&mut packet, 0b001);
        }
    }

    /// Reply to `gp` queries by sending a list of my peers.
    fn reply_getpeers(&mut self, switch_packet: &SwitchPacket, route_packet: &RoutePacket, handle: u32) {
        let mut nodes = Vec::new();
        {
            // Add myself
            let mut my_pk = [0u8; 32];
            my_pk.copy_from_slice(&self.my_pk.0);
            nodes.push(NodeData {
                public_key: my_pk,
                path: [0, 0, 0, 0, 0, 0, 0, 0b001],
                version: 18,
            });
        }
        for (peer_handle, &(path, ref inner_conn)) in self.inner_conns.iter() {
            if *peer_handle != handle {
                // If the peer is not the one asking for the list of peers,
                // add it to the list.
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&inner_conn.their_pk().0);
                nodes.push(NodeData {
                    public_key: pk,
                    path: path,
                    version: 18, // TODO
                });
                println!("Announcing one peer, with path: {}", path.to_vec().to_hex());
            }
        }
        // TODO: only send the peers closest to the specified target address.

        let encoding_scheme = EncodingScheme::from_iter(vec![EncodingSchemeForm { prefix: 0, bit_count: 3, prefix_length: 0 }].iter());
        let route_packet = RoutePacketBuilder::new(18, route_packet.transaction_id.clone())
                .nodes_vec(nodes)
                .encoding_index(0) // This switch uses only one encoding scheme
                .encoding_scheme(encoding_scheme)
                .finalize();
        let getpeers_response = DataPacket::new(1, &DataPayload::RoutePacket(route_packet));
        let responses: Vec<_>;
        {
            let &mut (_path, ref mut inner_conn) = self.inner_conns.get_mut(&handle).unwrap();
            let tmp = inner_conn.wrap_message_immediately(&getpeers_response.raw);
            responses = tmp.into_iter().map(|r| make_reply(&switch_packet, r, inner_conn)).collect();
        }
        for mut response in responses {
            self.send(&mut response, 0b001);
        }
    }


    fn ping_node(&mut self, node: &Node) {
        let node_pk = PublicKey::from_slice(node.public_key()).unwrap();
        let addr = publickey_to_ipv6addr(&node_pk);
        println!("Pinging node {}", Ipv6Addr::from(addr));
        let encoding_scheme = EncodingScheme::from_iter(vec![EncodingSchemeForm { prefix: 0, bit_count: 3, prefix_length: 0 }].iter());
        let route_packet = RoutePacketBuilder::new(18, b"blah".to_vec())
                .query("pn".to_owned())
                .encoding_index(0)
                .encoding_scheme(encoding_scheme)
                .target_address(vec![0, 0, 0, 0, 0, 0, 0, 0])
                .finalize();
        let ping_message = DataPacket::new(1, &DataPayload::RoutePacket(route_packet));
        self.send_message_to_node(node, ping_message);
    }

    fn try_connect_ping_target(&mut self, address: &Address) {
        println!("Trying to connect to {}", Ipv6Addr::from(address));
        let (node_opt, messages) = {
            let (node_opt, messages) = self.router.get_node(address, 42);
            let messages: Vec<_> = messages.into_iter().map(|(node, msg)| (node.clone(), msg)).collect();
            (node_opt.cloned(), messages)
        };
        if let Some(node) = node_opt {
            println!("Found node. pk: {}", PublicKey(*node.public_key()).to_base32());
            self.ping_nodes.push(node);
        };
        println!("{} router messages", messages.len());
        for (query_node, message) in messages {
            let message = DataPacket::new(1, &DataPayload::RoutePacket(message));
            self.send_message_to_node(&query_node, message);
        }
    }

    /// Sometimes (random) sends `pn` queries.
    fn random_ping_node(&mut self) {
        if rand::thread_rng().next_u32() > 0xafffffff || true {
            println!("Pinging nodes.");
            for address in self.ping_targets.clone() {
                if !self.address_to_handle.contains_key(&address) {
                    self.try_connect_ping_target(&address)
                }
            }
            for node in self.ping_nodes.clone() {
                self.ping_node(&node);
            }
        }
    }

    /// Called when a CryptoAuth message is received through an end-to-end
    /// session.
    fn on_inner_ca_message(&mut self, switch_packet: &SwitchPacket, handle: u32, ca_message: Vec<u8>) {
        let data_packet = DataPacket { raw: ca_message };

        println!("Received inner ca message from handle {} with path {:?}: {}", handle, switch_packet.label(), data_packet);

        // If it is a query, reply to it.
        match data_packet.payload().unwrap() {
            DataPayload::RoutePacket(route_packet) => {
                if route_packet.query == Some("gp".to_owned()) {
                    self.reply_getpeers(switch_packet, &route_packet, handle);
                }
                let (path, ref conn) = *self.inner_conns.get(&handle).unwrap();
                let node = Node::new(conn.their_pk().0, path, route_packet.protocol_version as u64);
                println!("Adding {} to store.", conn.their_pk().to_base32());
                let addr = publickey_to_ipv6addr(conn.their_pk()).into();
                self.router.update(addr, node);
            }
        }
    }

    /// Generates an unused handle number.
    fn gen_handle(&self) -> u32 {
        loop {
            let handle = rand::thread_rng().next_u32();
            if !self.inner_conns.contains_key(&handle) {
                return handle
            }
        };
    }

    /// Called when a switch packet is sent to the self interface
    fn on_self_interface_switch_packet(&mut self, switch_packet: &SwitchPacket) {
        match switch_packet.payload() {
            Some(SwitchPayload::Control(ControlPacket::Ping { opaque_data, .. })) => {
                // If it is a ping packet, just reply to it.
                let control_response = ControlPacket::Pong { version: 18, opaque_data: opaque_data };
                let mut packet_response = SwitchPacket::new_reply(switch_packet, SwitchPayload::Control(control_response));
                self.send(&mut packet_response, 0b001);

                self.random_send_switch_ping(switch_packet);
            },
            Some(SwitchPayload::Control(ControlPacket::Pong { opaque_data, .. })) => {
                // If it is a pong packet, print it.
                assert_eq!(opaque_data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
            },
            Some(SwitchPayload::CryptoAuthHandshake(handshake)) => {
                // If it is a CryptoAuth handshake packet (ie. if someone is
                // connecting to us), create a new session for this node.
                // All CA handshake we receive will be sessions started by
                // other peers, because this switch never starts sessions
                // (routers do, not switches).
                let handle = self.gen_handle();
                let (inner_conn, inner_packet) = Wrapper::new_incoming_connection(self.my_pk, self.my_sk.clone(), Credentials::None, None, Some(handle), handshake.clone()).unwrap();
                let path = {
                    let mut path = switch_packet.label();
                    reverse_label(&mut path);
                    path
                };
                self.address_to_handle.insert(publickey_to_ipv6addr(inner_conn.their_pk()).into(), handle);
                self.inner_conns.insert(handle, (path, inner_conn));
                self.on_inner_ca_message(switch_packet, handle, inner_packet);
                self.random_send_switch_ping(switch_packet);
            },
            Some(SwitchPayload::CryptoAuthData(handle, ca_message)) => {
                // If it is a CryptoAuth data packet, first read the session
                // handle to know which CryptoAuth session to use to
                // decrypt it.
                let inner_packets = match self.inner_conns.get_mut(&handle) {
                    Some(&mut (_path, ref mut inner_conn)) => {
                        match inner_conn.unwrap_message(ca_message) {
                            Ok(inner_packets) => inner_packets,
                            Err(e) => panic!("CA error: {:?}", e),
                        }
                    }
                    None => panic!("Received unknown handle.")
                };
                for inner_packet in inner_packets {
                    self.on_inner_ca_message(switch_packet, handle, inner_packet)
                }
            }
            _ => panic!("Can only handle Pings, Pongs, and CA."),
        }
    }

    // Find what interface a UDP packet is coming from, using its emitted
    // IP address.
    fn get_incoming_iface_and_open(&mut self, from_addr: SocketAddr, buf: Vec<u8>) -> (&Interface, Vec<Vec<u8>>) {
        let mut iface_exists = false;
        for candidate_interface in self.interfaces.iter_mut() {
            if candidate_interface.addr == from_addr {
                iface_exists = true;
                break
            }
        }

        if iface_exists {
            // Workaround for https://github.com/rust-lang/rust/issues/38614
            for candidate_interface in self.interfaces.iter_mut() {
                if candidate_interface.addr == from_addr {
                    let messages = candidate_interface.ca_session.unwrap_message(buf).unwrap();
                    return (candidate_interface, messages);
                }
            }
            panic!("The impossible happened.");
        }
        else {
            // Not a known interface; create one
            let next_iface_id = (0..0b1000).filter(|candidate| self.interfaces.iter().find(|iface| iface.id == *candidate).is_none()).next().unwrap();
            let (ca_session, message) = Wrapper::new_incoming_connection(self.my_pk.clone(), self.my_sk.clone(), Credentials::None, Some(self.allowed_peers.clone()), None, buf).unwrap();
            let new_iface = Interface { id: next_iface_id, ca_session: ca_session, addr: from_addr };
            self.interfaces.push(new_iface);
            let interface = self.interfaces.last_mut().unwrap();
            (interface, vec![message])
        }
    }

    /// Called when a UDP packet is received.
    fn on_outer_ca_message(&mut self, from_addr: SocketAddr, buf: Vec<u8>) {
        let (iface_id, messages) = {
            let (interface, messages) = self.get_incoming_iface_and_open(from_addr, buf);
            (interface.id, messages)
        };
        for message in messages {
            let mut switch_packet = SwitchPacket { raw: message };
            self.send(&mut switch_packet, iface_id)
        }
    }

    fn loop_(&mut self) {
        loop {
            for interface in self.interfaces.iter_mut() {
                for packet in interface.ca_session.upkeep() {
                    self.sock.send_to(&packet, interface.addr).unwrap();
                }
            }

            self.random_ping_node();

            let mut buf = vec![0u8; 4096];
            let (nb_bytes, addr) = self.sock.recv_from(&mut buf).unwrap();
            assert!(nb_bytes < 4096);
            buf.truncate(nb_bytes);
            self.on_outer_ca_message(addr, buf);
        }
    }
}

pub fn main() {
    fcp_cryptoauth::init();

    let my_sk = SecretKey::from_hex(b"ac3e53b518e68449692b0b2f2926ef2fdc1eac5b9dbd10a48114263b8c8ed12e").unwrap();
    let my_pk = PublicKey::from_base32(b"2wrpv8p4tjwm532sjxcbqzkp7kdwfwzzbg7g0n5l6g3s8df4kvv0.k").unwrap();
    let their_pk = PublicKey::from_base32(b"g0pt6kwnwj8ndktjhs7pmcl14rg6uugn8kt4nykudtl96r27sch0.k").unwrap();
    let login = "foo".to_owned().into_bytes();
    let password = "bar".to_owned().into_bytes();
    let credentials = Credentials::LoginPassword {
        login: login,
        password: password,
    };

    let mut allowed_peers = HashMap::new();
    allowed_peers.insert(credentials.clone(), "my peer".to_owned());

    let ping_targets = vec![
        Address::from(&Ipv6Addr::from_str("fcd6:9c33:dd06:3320:8dbe:ab19:c87:f6e3").unwrap()),
        Address::from(&Ipv6Addr::from_str("fcb9:326d:37d5:c57b:7ee5:28b5:7aa5:525").unwrap()),
        ];


    let sock = UdpSocket::bind("[::1]:12345").unwrap();
    let dest = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 20984);

    let conn = Wrapper::new_outgoing_connection(
            my_pk, my_sk.clone(), their_pk, credentials, Some(allowed_peers.clone()), "my peer".to_owned(), None);

    let interfaces = vec![Interface { id: 0b011, ca_session: conn, addr: dest }];

    let mut switch = Pinger::new(sock, interfaces, my_pk, my_sk, allowed_peers, ping_targets);

    switch.loop_();
}
