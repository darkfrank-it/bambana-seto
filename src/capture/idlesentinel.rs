use std::ffi::OsStr;
use std::time::Duration;
use sysinfo::{Pid, System}; // PidExt, ProcessExt, SystemExt
use tokio::task::JoinHandle;
use tokio::time;
use std::sync::{Arc, Mutex};

// use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, GetWindowTextW};
use windows::Win32::System::SystemInformation::{GetTickCount};
use winapi::um::winuser::{LASTINPUTINFO, GetLastInputInfo};

// use crate::draw::atena;
// use crate::db::thot;
// use crate::capture::window;


// pub async fn get_process(data_clone: Arc<Mutex<Vec<String>>>) {
//     let sys = System::new_all();

//     let (window_pid, window_title) = window::get_active_window();

//     // if window_pid == 0 {
//     // idle process
//     // return;
//     // }

//     let process = sys.processes().get(&Pid::from_u32(window_pid));

//     if let Some(process) = process {
//         let name = format_name(window_title.as_str(), process.name());

//         // here you'd do the work to store the name, timestamp, and so on
//         // println!("Process: {}, PID: {}, Window Title: {}",
//         //     name, window_pid, window_title
//         // );
//                 // let mut counter = 0;
//         // loop {
//             // {
//                 let mut data = data_clone.lock().unwrap();
//                 // for i in 0..5 {
//                 //     data[i] = format!("Riga {}: {}", i + 1, counter + i);
//                 // }
//             // }
//             data[i] = format!("Process: {}, PID: {}, Window Title: {}",
//                 name, window_pid, window_title
//             );
//             // atena::draw(&shared_data);
//             // counter += 1;
//             // time::sleep(Duration::from_secs(1)).await;
//         // }
//     }
// }

// fn format_name<'a>(window_name: &'a str, process_name: &'a OsStr) -> &'a str {
//     if process_name.eq("ApplicationFrameHost.exe") {
//         return window_name;
//     }

//     if let Some(s) = process_name.to_str() {
//         return s;
//     } else {
//         return window_name;
//     }
// }

pub fn get_last_input() -> Duration {
    let tick_count = unsafe { GetTickCount() };
    let mut last_input_info = LASTINPUTINFO {
        cbSize: 8, // Probably only true for 64 bit systems?
        dwTime: 0,
    };

    let p_last_input_info = &mut last_input_info as *mut LASTINPUTINFO;

    let _success = unsafe { GetLastInputInfo(p_last_input_info) };
    let diff = tick_count - last_input_info.dwTime;
    return Duration::from_millis(diff.into());
}


const IDLE_CHECK_SECS: i32 = 10;
const IDLE_PERIOD: u64 = 30;

pub fn track_processes(
    mut broadcast_rx: tokio::sync::broadcast::Receiver<String>,
    data_clone: Arc<Mutex<Vec<String>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(5));

        let mut i = 0;
        let mut idle = false;

        loop {
            i = i + 1;
            // println!("i {}", i);

            tokio::select! {
                Ok(cmd) = broadcast_rx.recv() => {
                    // println!("Task panoptes riceve: {}", cmd);

                    if cmd.trim() == "exit" {
                        println!("🟡 Task panoptes ricevuto comando di shutdown");
                        break;
                    }
                }
                _ = interval.tick() => {
                    // interval ticked, continue loop
                    if i == IDLE_CHECK_SECS {
                        // we check that the last time the user made any input
                        // was shorter ago than our idle period.
                        // if it wasn't, we pause tracking
                        let duration = get_last_input().as_secs();
                        // println!("duration {}", duration);
                        if IDLE_PERIOD > 0 && duration > IDLE_PERIOD {
                            idle = true;
                        } else {
                            idle = false;
                        }
                        i = 0;
                    }

                    // if !idle {
                    //     get_process( data_clone).await;
                    // }
                }
            }
        }
    })
}
