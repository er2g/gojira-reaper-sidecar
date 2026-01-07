mod main_loop;
mod net;
mod protocol;
mod reaper_api;
mod resolver;
mod validator;

use crate::main_loop::MainLoop;
use crate::net::NetworkThread;
use crate::protocol::{OutboundMsg, ServerMessage};
use crate::reaper_api::ReaperApiImpl;
use c_str_macro::c_str;
use crossbeam_channel::bounded;
use reaper_low::raw::{HINSTANCE, reaper_plugin_info_t};
use reaper_low::{Reaper, ReaperPluginContext};
use std::error::Error;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static REAPER: OnceLock<Reaper> = OnceLock::new();
static MAIN_LOOP: OnceLock<Mutex<MainLoop>> = OnceLock::new();
static NET_THREAD: OnceLock<NetworkThread> = OnceLock::new();
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn timer_proc() {
    let _ = reaper_low::firewall(|| {
        if SHUTDOWN.load(Ordering::Relaxed) {
            return;
        }
        let Some(reaper) = REAPER.get().copied() else {
            return;
        };
        let Some(main_loop) = MAIN_LOOP.get() else {
            return;
        };
        let Ok(mut main_loop) = main_loop.lock() else {
            return;
        };
        let api = ReaperApiImpl::new(reaper);
        main_loop.tick(&api);
    });
}

fn init(context: &ReaperPluginContext) -> Result<(), Box<dyn Error>> {
    let reaper = Reaper::load(context);
    let _ = REAPER.set(reaper);

    let (in_tx, in_rx) = bounded(protocol::INBOUND_CAP);
    let (out_tx, out_rx) = bounded(protocol::OUTBOUND_CAP);

    let net = NetworkThread::spawn(in_tx, out_rx)?;
    let _ = NET_THREAD.set(net);

    let main_loop = MainLoop::new(in_rx, out_tx);
    let _ = MAIN_LOOP.set(Mutex::new(main_loop));

    unsafe {
        reaper.plugin_register(c_str!("timer").as_ptr(), timer_proc as *mut c_void);
    }

    Ok(())
}

fn shutdown() {
    SHUTDOWN.store(true, Ordering::Relaxed);

    if let Some(net) = NET_THREAD.get() {
        net.shutdown();
    }

    if let Some(reaper) = REAPER.get().copied() {
        unsafe {
            // Prevent REAPER from calling into an unloaded DLL.
            let _ = reaper.plugin_register(c_str!("-timer").as_ptr(), timer_proc as *mut c_void);
        }
    }

    if let Some(main_loop) = MAIN_LOOP.get() {
        if let Ok(mut loop_guard) = main_loop.lock() {
            // Best-effort: send a final "server shutting down" error (will be dropped if no client).
            loop_guard.try_send(OutboundMsg::Send {
                msg: ServerMessage::Error {
                    msg: "server shutting down".to_string(),
                    code: protocol::ErrorCode::InternalError,
                },
            });
        }
    }
}

#[no_mangle]
pub extern "C" fn ReaperPluginEntry(
    h_instance: HINSTANCE,
    rec: *mut reaper_plugin_info_t,
) -> i32 {
    if rec.is_null() {
        shutdown();
        return 0;
    }
    reaper_low::bootstrap_extension_plugin(h_instance, rec, init)
}
