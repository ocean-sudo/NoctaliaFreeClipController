//! Find a FreeClip 2 that BlueZ already knows about.
//!
//! Pairing, trust, and the audio connection stay with the desktop Bluetooth
//! service. This only observes adapter state and later opens the vendor SPP
//! channel once the headset is connected.

use bluer::{Adapter, Address};

#[derive(Debug, Clone)]
pub struct FoundDevice {
    pub address: Address,
    pub label: String,
    pub paired: bool,
    pub connected: bool,
}

pub fn is_freeclip2(name: &str) -> bool {
    let folded = name.to_ascii_lowercase();
    folded.contains("freeclip 2") || folded.contains("freeclip2")
}

pub async fn find_device(
    adapter: &Adapter,
    wanted: Option<Address>,
) -> Result<Option<FoundDevice>, String> {
    let addresses = adapter
        .device_addresses()
        .await
        .map_err(|err| format!("listing bluetooth devices: {err}"))?;
    let mut matches = Vec::new();
    for address in addresses {
        if wanted.is_some_and(|wanted| wanted != address) {
            continue;
        }
        let device = adapter
            .device(address)
            .map_err(|err| format!("bluetooth device {address}: {err}"))?;
        let alias = device.alias().await.unwrap_or_default();
        let name = device.name().await.ok().flatten().unwrap_or_default();
        let label = if !alias.is_empty() {
            alias.clone()
        } else {
            name.clone()
        };
        if wanted.is_none() && !is_freeclip2(&alias) && !is_freeclip2(&name) {
            continue;
        }
        let paired = device.is_paired().await.unwrap_or(false);
        let connected = device.is_connected().await.unwrap_or(false);
        matches.push(FoundDevice {
            address,
            label,
            paired,
            connected,
        });
    }

    matches.sort_by_key(|device| !device.connected);
    Ok(matches.into_iter().next())
}
