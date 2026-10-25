use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};

use mdns_sd::{IfKind, ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::watch;

use crate::error::RemoteError;
use crate::lan::is_lan_peer;
use crate::protocol::{DeviceId, DeviceInfo, PROTOCOL_VERSION, SERVICE_TYPE};

const ID_KEY: &str = "id";
const NAME_KEY: &str = "name";
const VERSION_KEY: &str = "v";
const MAX_DISCOVERED_DEVICES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    pub device: DeviceInfo,
    pub address: SocketAddr,
}

pub struct Discovery {
    daemon: ServiceDaemon,
    advertised: Mutex<Option<String>>,
}

impl Discovery {
    pub fn start() -> Result<Self, RemoteError> {
        let daemon = ServiceDaemon::new().map_err(discovery_error)?;
        daemon
            .disable_interface(IfKind::IPv6)
            .map_err(discovery_error)?;
        Ok(Self {
            daemon,
            advertised: Mutex::new(None),
        })
    }

    pub fn advertise(&self, device: &DeviceInfo, port: u16) -> Result<(), RemoteError> {
        self.withdraw();
        let properties = [
            (ID_KEY, device.id.0.as_str()),
            (NAME_KEY, device.name.as_str()),
            (VERSION_KEY, &PROTOCOL_VERSION.to_string()),
        ];
        let instance = format!("{} ({})", device.name, device.id.0);
        let host_name = format!("cadence-{}.local.", device.id.0);
        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &host_name,
            "",
            port,
            &properties[..],
        )
        .map_err(discovery_error)?
        .enable_addr_auto();
        let fullname = service.get_fullname().to_owned();
        self.daemon.register(service).map_err(discovery_error)?;
        *self
            .advertised
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(fullname);
        Ok(())
    }

    pub fn withdraw(&self) {
        let Some(fullname) = self
            .advertised
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        else {
            return;
        };
        if let Err(error) = self.daemon.unregister(&fullname) {
            tracing::debug!("withdrawing the remote control announcement failed: {error}");
        }
    }

    pub fn browse(&self, me: DeviceId) -> Result<watch::Receiver<Vec<Discovered>>, RemoteError> {
        let events = self.daemon.browse(SERVICE_TYPE).map_err(discovery_error)?;
        let (sender, receiver) = watch::channel(Vec::new());
        tokio::spawn(async move {
            let mut found: HashMap<String, Discovered> = HashMap::new();
            while let Ok(event) = events.recv_async().await {
                let changed = match event {
                    ServiceEvent::ServiceResolved(service) => discovered_of(&service, &me)
                        .is_some_and(|device| {
                            remember(&mut found, service.get_fullname().to_owned(), device)
                        }),
                    ServiceEvent::ServiceRemoved(_service_type, fullname) => {
                        found.remove(&fullname).is_some()
                    }
                    _ => false,
                };
                if changed && sender.send(sorted(&found)).is_err() {
                    return;
                }
            }
        });
        Ok(receiver)
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.withdraw();
        if let Err(error) = self.daemon.shutdown() {
            tracing::debug!("stopping device discovery failed: {error}");
        }
    }
}

fn remember(found: &mut HashMap<String, Discovered>, fullname: String, device: Discovered) -> bool {
    if found.len() >= MAX_DISCOVERED_DEVICES && !found.contains_key(&fullname) {
        tracing::debug!("ignoring a player announcement past {MAX_DISCOVERED_DEVICES} players");
        return false;
    }
    found.insert(fullname, device);
    true
}

fn sorted(found: &HashMap<String, Discovered>) -> Vec<Discovered> {
    let mut devices: Vec<Discovered> = found.values().cloned().collect();
    devices.sort_by(|left, right| {
        left.device
            .name
            .cmp(&right.device.name)
            .then(left.device.id.0.cmp(&right.device.id.0))
    });
    devices.dedup_by(|later, earlier| later.device.id == earlier.device.id);
    devices
}

fn discovered_of(service: &ResolvedService, me: &DeviceId) -> Option<Discovered> {
    let id = service.get_property_val_str(ID_KEY)?;
    let name = service.get_property_val_str(NAME_KEY).unwrap_or(id);
    let version = service
        .get_property_val_str(VERSION_KEY)?
        .parse::<u16>()
        .ok()?;
    if id == me.0 || version != PROTOCOL_VERSION {
        return None;
    }
    let address = reachable_address(service.get_addresses_v4().into_iter().map(IpAddr::V4))?;
    Some(Discovered {
        device: DeviceInfo {
            id: DeviceId(id.to_owned()),
            name: name.to_owned(),
        }
        .cleaned(),
        address: SocketAddr::new(address, service.get_port()),
    })
}

fn reachable_address(addresses: impl Iterator<Item = IpAddr>) -> Option<IpAddr> {
    addresses
        .filter(|address| is_lan_peer(*address))
        .min_by_key(|address| (address.is_loopback(), *address))
}

fn discovery_error(error: mdns_sd::Error) -> RemoteError {
    RemoteError::Discovery(Arc::new(error))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::pairing::new_device_id;

    use super::*;

    const ANNOUNCEMENT_TIMEOUT: Duration = Duration::from_secs(15);
    const ADVERTISED_PORT: u16 = 4017;
    const LAN_ADDRESS: &str = "192.168.1.7";

    fn announcement(properties: &[(&str, &str)], address: &str) -> ResolvedService {
        ServiceInfo::new(
            SERVICE_TYPE,
            "Kitchen (player-2)",
            "cadence-player-2.local.",
            address,
            ADVERTISED_PORT,
            properties,
        )
        .unwrap()
        .as_resolved_service()
    }

    fn me() -> DeviceId {
        DeviceId("me-1".to_owned())
    }

    fn version() -> String {
        PROTOCOL_VERSION.to_string()
    }

    async fn wait_until(
        receiver: &mut watch::Receiver<Vec<Discovered>>,
        is_done: impl FnMut(&Vec<Discovered>) -> bool,
    ) -> Vec<Discovered> {
        tokio::time::timeout(ANNOUNCEMENT_TIMEOUT, receiver.wait_for(is_done))
            .await
            .unwrap()
            .unwrap()
            .clone()
    }

    #[tokio::test]
    async fn given_another_player_advertises_when_browsing_then_it_is_found_until_withdrawn() {
        let advertiser = Discovery::start().unwrap();
        let browser = Discovery::start().unwrap();
        let player = DeviceInfo {
            id: DeviceId(new_device_id()),
            name: "Living Room".to_owned(),
        };
        let mut devices = browser.browse(DeviceId(new_device_id())).unwrap();
        advertiser.advertise(&player, ADVERTISED_PORT).unwrap();

        let found = wait_until(&mut devices, |devices| {
            devices.iter().any(|found| found.device.id == player.id)
        })
        .await;
        let found_player = found
            .iter()
            .find(|found| found.device.id == player.id)
            .unwrap();
        assert_eq!(found_player.device, player);
        assert_eq!(found_player.address.port(), ADVERTISED_PORT);
        assert!(is_lan_peer(found_player.address.ip()));

        advertiser.withdraw();
        wait_until(&mut devices, |devices| {
            devices.iter().all(|found| found.device.id != player.id)
        })
        .await;
    }

    #[test]
    fn given_a_complete_announcement_when_read_then_it_is_the_cleaned_device_and_its_address() {
        let version = version();
        let service = announcement(
            &[
                (ID_KEY, "player-2"),
                (NAME_KEY, " Kitchen\u{7}"),
                (VERSION_KEY, &version),
            ],
            LAN_ADDRESS,
        );
        assert_eq!(
            discovered_of(&service, &me()),
            Some(Discovered {
                device: DeviceInfo {
                    id: DeviceId("player-2".to_owned()),
                    name: "Kitchen".to_owned(),
                },
                address: SocketAddr::new(LAN_ADDRESS.parse().unwrap(), ADVERTISED_PORT),
            })
        );
    }

    #[test]
    fn given_an_announcement_without_a_name_when_read_then_the_id_is_the_name() {
        let version = version();
        let service = announcement(
            &[(ID_KEY, "player-2"), (VERSION_KEY, &version)],
            LAN_ADDRESS,
        );
        let found = discovered_of(&service, &me()).unwrap();
        assert_eq!(found.device.name, "player-2");
    }

    #[test]
    fn given_an_unusable_announcement_when_read_then_it_is_ignored() {
        let version = version();
        let other_version = (PROTOCOL_VERSION + 1).to_string();
        let unusable = [
            announcement(
                &[(NAME_KEY, "Kitchen"), (VERSION_KEY, &version)],
                LAN_ADDRESS,
            ),
            announcement(&[(ID_KEY, "player-2"), (NAME_KEY, "Kitchen")], LAN_ADDRESS),
            announcement(&[(ID_KEY, "player-2"), (VERSION_KEY, "one")], LAN_ADDRESS),
            announcement(
                &[(ID_KEY, "player-2"), (VERSION_KEY, &other_version)],
                LAN_ADDRESS,
            ),
            announcement(&[(ID_KEY, "me-1"), (VERSION_KEY, &version)], LAN_ADDRESS),
            announcement(&[(ID_KEY, "player-2"), (VERSION_KEY, &version)], "8.8.8.8"),
        ];
        for service in unusable {
            assert_eq!(discovered_of(&service, &me()), None, "{service:?}");
        }
    }

    #[test]
    fn given_found_players_when_listed_then_they_are_sorted_by_name_without_repeated_ids() {
        let mut found = HashMap::new();
        found.insert("b".to_owned(), discovered(2));
        found.insert("a".to_owned(), discovered(1));
        found.insert("a-again".to_owned(), discovered(1));
        assert_eq!(sorted(&found), vec![discovered(1), discovered(2)]);
    }

    fn discovered(index: usize) -> Discovered {
        Discovered {
            device: DeviceInfo {
                id: DeviceId(format!("player-{index}")),
                name: format!("Player {index}"),
            },
            address: SocketAddr::new(IpAddr::from([192, 168, 1, 7]), 4000),
        }
    }

    #[test]
    fn given_more_announcements_than_the_cap_only_known_players_are_kept() {
        let mut found = HashMap::new();
        for index in 0..MAX_DISCOVERED_DEVICES {
            assert!(remember(
                &mut found,
                format!("player-{index}"),
                discovered(index)
            ));
        }
        let late = MAX_DISCOVERED_DEVICES;
        assert!(!remember(
            &mut found,
            format!("player-{late}"),
            discovered(late)
        ));
        assert!(remember(
            &mut found,
            "player-0".to_owned(),
            discovered(late)
        ));
        assert_eq!(found.len(), MAX_DISCOVERED_DEVICES);
    }

    #[test]
    fn a_network_address_is_preferred_over_loopback() {
        let addresses =
            ["127.0.0.1", "8.8.8.8", "192.168.1.7"].map(|text| text.parse::<IpAddr>().unwrap());
        assert_eq!(
            reachable_address(addresses.into_iter()),
            Some("192.168.1.7".parse().unwrap())
        );
        let only_loopback = ["127.0.0.1".parse::<IpAddr>().unwrap()];
        assert_eq!(
            reachable_address(only_loopback.into_iter()),
            Some("127.0.0.1".parse().unwrap())
        );
        assert_eq!(
            reachable_address(["8.8.8.8".parse::<IpAddr>().unwrap()].into_iter()),
            None
        );
    }
}
