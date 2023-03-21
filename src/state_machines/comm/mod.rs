use defmt::{info, Format};
use embassy_time::{with_timeout, Duration};

use crate::{
    tasks::DFU_DONE,
    types::{Error, RawPacket, ResponsePayload, RX_BUS, TX_BUS},
};

use super::dfu::types::{DfuState, DfuStateMachine};

#[derive(Format)]
enum CommState {
    Idle,
    DfuOngoing,
    ConfigOngoing,
    LoggingOngoing,
}

struct CommStateMachine {
    state: CommState,
    timeout: Duration,
    error_counter: u8,
    dfu_sm: DfuStateMachine,
}

impl CommStateMachine {
    fn new(dfu_sm: DfuStateMachine) -> Self {
        Self {
            state: CommState::Idle,
            timeout: Duration::from_ticks(u32::MAX.into()),
            error_counter: 0,
            dfu_sm,
        }
    }

    fn reset(&mut self) {
        defmt::warn!("Resetting COMM state machine");
        self.state = CommState::Idle;
        self.timeout = Duration::from_ticks(u32::MAX.into());
        self.error_counter = 0;
        self.dfu_sm = DfuStateMachine::new();
    }

    async fn tick(&mut self, packet: RawPacket) -> Result<ResponsePayload, Error> {
        let response = ResponsePayload::None;
        match self.state {
            CommState::Idle => match packet {
                RawPacket::RecvDfuHeader(dfu_header) => {
                    self.dfu_sm.init(dfu_header);
                    // Se the timeout of the state machine to 100ms for DFU mode.
                    self.timeout = Duration::from_millis(100);
                    self.state = CommState::DfuOngoing;
                }
                RawPacket::RecvConfigHeader(config_header) => {
                    todo!();
                    // self.state = CommState::ConfigOngoing;
                }
                RawPacket::RecvLoggingHeader(logging_header) => {
                    todo!();
                    // self.state = CommState::LoggingOngoing;
                }
                _ => {
                    // Do nothing. We wait for a start operation. To activate another state machine.
                }
            },
            CommState::DfuOngoing => {
                // Every state machine will report back a response and wether it is done or not.
                let res = self.dfu_sm.tick(packet).await?;
                if let DfuState::Done = res {
                    DFU_DONE.signal(true);
                    self.reset();
                }
            }
            CommState::ConfigOngoing => todo!(),
            CommState::LoggingOngoing => todo!(),
        };

        Ok(response)
    }
}

/// This is the main Communication State Machine. It handles everything communication-related.
/// Data comes in via a subscriber and gets sent away via a publisher.
pub async fn run() {
    let mut data_rx = RX_BUS
        .dyn_subscriber()
        .expect("Failed to acquire subscriber.");
    let data_tx = TX_BUS
        .dyn_publisher()
        .expect("Failed to acquire publisher.");

    let dfu_sm = DfuStateMachine::new();
    let mut sm = CommStateMachine::new(dfu_sm);
    loop {
        match with_timeout(sm.timeout, data_rx.next_message_pure())
            .await
            .map_err(|_| Error::CommTimeout)
            .flatten()
        {
            Ok(packet) => {
                // Clear error counter
                sm.error_counter = 0;
                let res = sm.tick(packet).await;
                match res {
                    Ok(response) => {
                        data_tx.publish(RawPacket::RespOK(response)).await;
                    }
                    // Non-recoverable error!
                    Err(_e) => {
                        sm.reset();
                    }
                };
            }
            Err(e) => {
                // defmt::error!("Timeout or something else: {:?}", e);
                // Timeout or some other Physical error.
                // Send a NOK. This causes CLI to repeat.
                sm.error_counter += 1;
                data_tx.publish(RawPacket::RespNOK(None)).await;
            }
        };

        // In the case of five errors one after another.
        if sm.error_counter >= 3 {
            defmt::error!("Communication error.");
            sm.reset();
        }
    }
}
