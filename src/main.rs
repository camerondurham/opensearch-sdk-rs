use std::io::Read;
use std::net::{Ipv4Addr, TcpListener};

use opensearch_sdk_rs::transport::TransportMessage;

const DEFAULT_PORT: u32 = 7878;

#[derive(Debug)]
pub struct Host {
    address: Ipv4Addr,
    port: u32,
}

impl Host {
    pub fn new(port: u32) -> Host {
        Host {
            address: Ipv4Addr::new(127, 0, 0, 1),
            port,
        }
    }
    pub fn default() -> Host {
        Host {
            address: Ipv4Addr::new(127, 0, 0, 1),
            port: DEFAULT_PORT,
        }
    }

    fn handle_handshake(stream: &mut std::net::TcpStream, request: &TransportMessage, count: i32) {
        println!(
            "[{}] Handshake request from OpenSearch version: {}",
            count, request.header.version
        );

        // Create handshake response with same request_id and version
        let response = TransportMessage::create_handshake_response(
            request.header.request_id,
            request.header.version,
        );

        // Send handshake response
        match response.write_to_stream(stream) {
            Ok(_) => {
                println!("[{}] Handshake response sent successfully", count);
            }
            Err(e) => {
                eprintln!("[{}] Error sending handshake response: {:?}", count, e);
            }
        }
    }
    pub fn run(&self) {
        let listener = TcpListener::bind(format!("{}:{}", &self.address.to_string(), &self.port))
            .expect(&format!("Unable to bind to port: {}", &self.port));
        let mut count = 0;

        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error accepting connection: {:?}", e);
                    continue;
                }
            };
            count += 1;

            println!(
                "[{}] Received connection from {:?}",
                &count,
                &stream.peer_addr()
            );

            match TransportMessage::from_stream(&mut stream) {
                Ok(message) => {
                    println!(
                        "[{}] Parsed message - request_id: {}, status: {}, is_handshake: {}",
                        &count,
                        message.header.request_id,
                        message.header.status,
                        message.is_handshake()
                    );

                    if message.is_handshake() {
                        println!("[{}] Handling handshake request", &count);
                        Self::handle_handshake(&mut stream, &message, count);
                    } else if message.is_request_response() {
                        println!("[{}] Handling request/response", &count);
                        // TODO: Handle other request types
                    } else {
                        println!(
                            "[{}] Unknown message type with status: {}",
                            &count, message.header.status
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[{}] Error parsing message: {:?}", count, e);
                }
            }
        }
    }
}

fn main() {
    let host = Host::new(1234);
    host.run();

    // register the "hello" extension with command line
    // curl -XPOST "localhost:9200/_extensions/initialize" -H "Content-Type:application/json" --data @hello.json
}
