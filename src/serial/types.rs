use defmt::Format;

#[allow(clippy::enum_variant_names)]
#[derive(Format, Debug, Clone, Copy)]
pub enum UartError {
    /// Error at the physical layer (UART or BLE).
    UartRx,
    UartTx,
    UartBufferFull,
}
