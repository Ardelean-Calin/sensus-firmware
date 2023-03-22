// use crate::types::{ErrorResponse, RawPacket, ResponsePayload};

// pub struct LogStateMachine {}
// impl LogStateMachine {
//     pub fn new() -> Self {
//         Self {}
//     }

//     pub async fn tick(&mut self, packet: RawPacket) -> Result<ResponsePayload, ErrorResponse> {
//         match packet {
//             RawPacket::RecvLoggingHeader(false) => {
//                 // TODO. Disable logging.
//                 todo!();
//             }
//             RawPacket::LogGetLatest => todo!(),
//             _ => {
//                 todo!()
//             }
//         }
//     }
// }
