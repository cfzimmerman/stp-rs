use anyhow::bail;
use std::time::Duration;
use stp_rs::stp::eth::EthSwitch;

/// How often switches broadcast their routing state to neighbors
const BPDU_RESEND_FREQ: Duration = Duration::from_secs(2);

/// How long a switch is allowed to wait for an ethernet packet to
/// arrive on a specific port. All relevant ports are polled in an
/// event loop.
const SWITCH_TICK_SPEED: Option<Duration> = Some(Duration::from_micros(1000));

fn main() -> anyhow::Result<()> {
    let Some(switch_name) = std::env::args().nth(1) else {
        bail!("First argument must be the switch name");
    };
    let switch = EthSwitch::build(&switch_name, BPDU_RESEND_FREQ, SWITCH_TICK_SPEED)?;
    switch.run(Duration::from_millis(500))
}
