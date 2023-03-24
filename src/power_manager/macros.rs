#[macro_export]
macro_rules! run_while_plugged_in {
    ($guard:expr, $task:expr) => {{
        async move {
            let mut _sub_plugged_in = $guard.plugged_in.subscriber().unwrap();
            let mut _sub_plugged_out = $guard.plugged_out.subscriber().unwrap();
            loop {
                let task = $task;
                let guard_leave = _sub_plugged_out.next_message_pure();

                // Wait for the guard to enter our context
                _sub_plugged_in.next_message_pure().await;
                // Once guard has entered our context, wait for it to go out of scope
                embassy_futures::select::select(guard_leave, task).await;
            }
        }
    }};
}
