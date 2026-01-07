use std::net::SocketAddr;

pub const INBOUND_CAP: usize = 256;
pub const OUTBOUND_CAP: usize = 256;

pub enum InboundMsg {
    ClientConnected {
        socket_addr: SocketAddr,
        session_token: String,
    },
    ClientDisconnected,
    Command { cmd: ClientCommand },
}

pub enum OutboundMsg {
    Send { msg: ServerMessage },
}

pub use gojira_protocol::{
    ClientCommand, Confidence, ErrorCode, GojiraInstance, MergeMode, ParamChange, ServerMessage,
};
