use defmt::Format;
use serde::Serialize;

use crate::{config::types::SensusConfig, dfu::types::DfuError, types::PacketError};

#[derive(Serialize, Format, Clone)]
pub enum CommResponse {
    OK(ResponseTypeOk),
    NOK(ResponseTypeErr),
}

// pub enum ResponseType {
//     ResponseTypeOk = {
//         Dfu(DfuOkType),
//         Config,
//         Log,}
// }
#[derive(Serialize, Format, Clone)]
pub enum ResponseTypeOk {
    NoData,
    Dfu(DfuResponse),
    Config(SensusConfig),
    Log,
}

#[derive(Serialize, Format, Clone)]
pub enum ResponseTypeErr {
    Packet(PacketError),
    Dfu(DfuError),
    Config,
}

#[derive(Serialize, Format, Clone)]
pub enum DfuResponse {
    FirmwareVersion(&'static str),
    NextBlock,
    DfuDone,
}
