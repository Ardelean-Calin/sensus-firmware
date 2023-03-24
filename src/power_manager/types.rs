use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, pubsub::PubSubChannel};

pub struct PowerDetect {
    pub plugged_in: PubSubChannel<ThreadModeRawMutex, bool, 1, 2, 1>,
    pub plugged_out: PubSubChannel<ThreadModeRawMutex, bool, 1, 2, 1>,
}
