use cadence_api::Result;
use cadence_core::model::LibraryScan;

use super::SubsonicServer;
use crate::client::Params;
use crate::types::{ScanStatus, ScanStatusResponse};

fn library_scan_of(status: ScanStatus) -> LibraryScan {
    LibraryScan {
        is_scanning: status.is_scanning,
        scanned_items: status
            .count
            .filter(|count| *count >= 0.0)
            .map(|count| count as u64),
    }
}

impl SubsonicServer {
    pub(super) async fn begin_library_scan(&self) -> Result<LibraryScan> {
        let scan = self.scan_status("startScan.view").await?;
        self.forget_cached_lists_after(&scan);
        Ok(scan)
    }

    pub(super) async fn fetch_library_scan(&self) -> Result<LibraryScan> {
        let scan = self.scan_status("getScanStatus.view").await?;
        self.forget_cached_lists_after(&scan);
        Ok(scan)
    }

    async fn scan_status(&self, endpoint: &str) -> Result<LibraryScan> {
        let response: ScanStatusResponse = self.fetch(endpoint, Params::new()).await?;
        Ok(library_scan_of(response.scan_status.unwrap_or_default()))
    }

    fn forget_cached_lists_after(&self, scan: &LibraryScan) {
        if scan.is_scanning {
            return;
        }
        self.list_caches.forget_annotations();
        self.list_caches.forget_playlists();
        self.latest_folder.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_counts_are_ignored() {
        let status = ScanStatus {
            is_scanning: true,
            count: Some(-1.0),
        };
        assert_eq!(
            library_scan_of(status),
            LibraryScan {
                is_scanning: true,
                scanned_items: None
            }
        );
    }

    #[test]
    fn counts_become_scanned_items() {
        let status = ScanStatus {
            is_scanning: false,
            count: Some(1234.0),
        };
        assert_eq!(library_scan_of(status).scanned_items, Some(1234));
    }
}
