use anyhow::bail;
use std::time::Duration;
use stp_rs::stp::eth::EthRouter;

fn main() -> anyhow::Result<()> {
    let Some(switch_name) = std::env::args().into_iter().nth(1) else {
        bail!("First argument must be the switch name");
    };
    let switch = EthRouter::build(
        &switch_name,
        Duration::from_secs(10),
        Some(Duration::from_micros(1000)),
    )?;
    Ok(switch.run(Duration::from_millis(500))?)
}
