use rustc_serialize::{Decodable, Decoder};

#[derive(Debug)]
#[derive(Eq)]
#[derive(PartialEq)]
pub enum Message {
    FindNode { target_address: String, transaction_id: String },
    Node { nodes: String, transaction_id: String },
}

impl Decodable for Message {
    fn decode<D: Decoder>(d: &mut D) -> Result<Message, D::Error> {
        d.read_struct("Message", 3, |d| {
            let query_res = d.read_struct_field("q", 0, D::read_str);
            match query_res {
                Ok(query) => {
                    match query.as_ref() {
                        "fn" => {
                            let target_address = try!(d.read_struct_field("tar", 0, D::read_str));
                            let transaction_id = try!(d.read_struct_field("txid", 1, D::read_str));
                            Ok(Message::FindNode { target_address: target_address, transaction_id: transaction_id })
                        },
                        _ => Err(d.error(&format!("Unknown query type: {}", query))),
                    }
                },
                Err(_) => { // 'q' is not given (or not readable, TODO: fix this)
                    let nodes = try!(d.read_struct_field("n", 0, D::read_str));
                    let transaction_id = try!(d.read_struct_field("txid", 1, D::read_str));
                    Ok(Message::Node { nodes: nodes, transaction_id: transaction_id })
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use bencode;
    use super::*;
    use rustc_serialize::Decodable;

    #[test]
    fn test_fn() {
        let s = "d1:q2:fn3:tar16:abcdefghhijklmno4:txid5:12345e".as_bytes();
        let mut bencode = bencode::from_buffer(s).unwrap();
        let mut decoder = bencode::Decoder::new(&mut bencode);
        let result = Message::decode(&mut decoder);
        let expected = Message::FindNode {
            target_address: "abcdefghhijklmno".to_owned(),
            transaction_id: "12345".to_owned()
        };
        assert_eq!(result, Ok(expected));
    }

    #[test]
    fn test_n() {
        let s = "d1:n80:cdefghijklmnopqrstuvwxyzabcdefghi1234567qponmlkjihgzyxwvutsrstuvwxyzabcde23456784:txid5:12345e".as_bytes();
        let mut bencode = bencode::from_buffer(s).unwrap();
        let mut decoder = bencode::Decoder::new(&mut bencode);
        let result = Message::decode(&mut decoder);
        let expected = Message::Node {
            nodes: "cdefghijklmnopqrstuvwxyzabcdefghi1234567qponmlkjihgzyxwvutsrstuvwxyzabcde2345678".to_owned(),
            transaction_id: "12345".to_owned()
        };
        assert_eq!(result, Ok(expected));
    }
}
