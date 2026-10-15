// SPDX-FileCopyrightText: 2026 roolrz
// SPDX-License-Identifier: Apache-2.0

//! One-shot retirement authority: stop only predecessors explicitly handed over,
//! drain their console and detach the old mount after all backing references end.

use hyper_bootstrap_handoff::retirement as contract;
use hyper_os::{capability_channel::*, handle::*, task::ProcessPhase};
use std::{mem::MaybeUninit, time::Duration};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut startup = hyper_rt::process::startup()?;
    let count = std::env::args()
        .nth(1)
        .ok_or("missing process count")?
        .parse::<usize>()?;
    if count > contract::MAX_PREDECESSORS {
        return Err("too many predecessors".into());
    }
    let endpoint = CapabilityChannel::from_handle(startup.take(contract::ENDPOINT)?);
    let mut slots = vec![
        CapabilityReceiveSlot::new::<DirectoryObject>(contract::ROOT),
        CapabilityReceiveSlot::new::<TaskGroupObject>(contract::GROUP),
        CapabilityReceiveSlot::new::<ByteChannelObject>(contract::DRAIN),
    ];
    for _ in 0..count {
        slots.push(CapabilityReceiveSlot::new::<ProcessObject>(
            contract::PROCESS,
        ));
    }
    let mut bytes = [MaybeUninit::uninit(); 2048];
    let message = endpoint.receive(hyper_service_ipc::deadline()?, &mut bytes, &mut slots)?;
    let offer: contract::Offer = serde_json::from_slice(message.bytes())?;
    if offer.processes != count
        || message.capability_count() != slots.len()
        || !offer.path.starts_with('/')
        || offer.path.len() > 256
        || offer.path.bytes().any(|b| b.is_ascii_control())
        || offer.path[1..]
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || offer.console.is_some_and(|i| i >= count)
    {
        return Err("invalid retirement inventory".into());
    }
    let root = slots[0]
        .take::<DirectoryObject>()?
        .ok_or("missing namespace")?;
    let group = slots[1]
        .take::<TaskGroupObject>()?
        .ok_or("missing predecessor group")?;
    let drain = slots[2]
        .take::<ByteChannelObject>()?
        .ok_or("missing drain channel")?;
    let processes = slots[3..]
        .iter_mut()
        .map(|s| {
            s.take::<ProcessObject>()?
                .ok_or(hyper_os::Error::MissingHandle)
        })
        .collect::<hyper_os::Result<Vec<_>>>()?;
    let until = hyper_os::time::deadline_after(Duration::from_secs(60))?.as_raw();
    for (i, p) in processes.iter().enumerate() {
        if offer.console != Some(i) {
            p.as_process_supervisor().request_stop()?;
        }
    }
    for (i, p) in processes.iter().enumerate() {
        if offer.console != Some(i) {
            wait_retired(p, until)?;
        }
    }
    if offer.drain {
        drain.as_byte_channel().send(b"HYPER-CONSOLE-DRAIN/1")?;
        use hyper_os::wait::*;
        wait_many(
            &[WaitItem::new(
                drain.as_handle_ref(),
                ObjectSignals::<ByteChannelObject>::READABLE
                    .union(ObjectSignals::<ByteChannelObject>::PEER_CLOSED),
            )],
            until,
        )?;
        let mut reply = [0; 64];
        let n = drain.as_byte_channel().try_receive(&mut reply)?;
        if &reply[..n] != b"HYPER-CONSOLE-DRAINED/1" {
            return Err("invalid console drain acknowledgement".into());
        }
    }
    group.as_task_group().request_stop()?;
    group.as_task_group().wait_terminated(until)?;
    for p in &processes {
        wait_retired(p, until)?;
    }
    drop(processes);
    drop(drain);
    drop(group);
    loop {
        match hyper_os::filesystem::unmount(root.as_handle_ref(), &offer.path) {
            Ok(()) => break,
            Err(hyper_os::Error::Status(hyper_os::Status::BUSY))
                if hyper_os::time::monotonic_now()?.as_nanoseconds() < until =>
            {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(e) => return Err(e.into()),
        }
    }
    println!("HypeR bootstrap-retire: bootstrap retired; boot archive detached");
    Ok(())
}
fn wait_retired(process: &OwnedHandle<ProcessObject>, until: u64) -> hyper_os::Result<()> {
    while process.as_process_supervisor().info()?.phase != ProcessPhase::Retired {
        if hyper_os::time::monotonic_now()?.as_nanoseconds() >= until {
            return Err(hyper_os::Error::Status(hyper_os::Status::TIMED_OUT));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("HypeR bootstrap-retire: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
