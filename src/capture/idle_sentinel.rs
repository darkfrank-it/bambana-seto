use std::mem::size_of;
use chrono::{DateTime, Duration, Utc};
use tokio::task::JoinHandle;
use tokio::time;

use windows::Win32::System::SystemInformation::GetTickCount;
use winapi::um::winuser::{GetLastInputInfo, LASTINPUTINFO};

pub fn get_last_input() -> Duration {
    let tick_count = unsafe { GetTickCount() };
    let mut last_input_info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };

    let p_last_input_info = &mut last_input_info as *mut LASTINPUTINFO;
    let _ = unsafe { GetLastInputInfo(p_last_input_info) };
    let diff = tick_count.saturating_sub(last_input_info.dwTime);
    Duration::milliseconds(diff as i64)
}

const IDLE_CHECK_SECS: u64 = 1;
const IDLE_PERIOD_SECS: u64 = 10;

pub fn start_idle_watcher(
    idle_return_tx: tokio::sync::mpsc::UnboundedSender<Duration>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(std::time::Duration::from_secs(IDLE_CHECK_SECS));
        let mut was_idle = false;
        let mut idle_start: Option<DateTime<Utc>> = None;

        loop {
            interval.tick().await;
            let duration_secs = get_last_input().num_seconds();
            let idle_duration = Duration::seconds(duration_secs);

            if idle_duration >= Duration::seconds(IDLE_PERIOD_SECS as i64) {
                if !was_idle {
                    was_idle = true;
                    idle_start = Some(Utc::now() - idle_duration);
                }
            } else if was_idle {
                let offline_duration = idle_start
                    .map(|start| Utc::now() - start)
                    .unwrap_or_else(|| Duration::seconds(duration_secs));
                let _ = idle_return_tx.send(offline_duration);
                was_idle = false;
                idle_start = None;
            }
        }
    })
}
