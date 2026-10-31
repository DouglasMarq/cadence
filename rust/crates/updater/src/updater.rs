use std::path::Path;
use std::sync::Arc;

use semver::Version;

use crate::download::Download;
use crate::error::{Result, UpdateError, io_error};
use crate::feed::ReleaseFeed;
use crate::installer::{Installer, Launch, installer_for};
use crate::platform::Platform;
use crate::relaunch::Relaunch;
use crate::release::Offer;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Downloading(u8),
    Installing,
}

pub struct Updater {
    feed: ReleaseFeed,
    platform: Platform,
    current: Version,
    installer: Option<Arc<dyn Installer>>,
}

impl Updater {
    pub fn new(
        feed: ReleaseFeed,
        platform: Platform,
        current: Version,
        installer: Option<Arc<dyn Installer>>,
    ) -> Self {
        Self {
            feed,
            platform,
            current,
            installer,
        }
    }

    pub async fn for_this_install() -> Result<Self> {
        let installer = tokio::task::spawn_blocking(detect_installer).await??;
        let current =
            Version::parse(CURRENT_VERSION).map_err(|source| UpdateError::InvalidVersion {
                tag: CURRENT_VERSION.to_owned(),
                source,
            })?;
        Ok(Self::new(
            ReleaseFeed::github()?,
            Platform::current(),
            current,
            installer,
        ))
    }

    pub fn can_install(&self) -> bool {
        self.installer.is_some()
    }

    pub async fn check(&self) -> Result<Option<Offer>> {
        let Some(release) = self.feed.latest().await? else {
            return Ok(None);
        };
        release.offer(&self.current, self.platform)
    }

    pub async fn install(
        &self,
        offer: &Offer,
        on_progress: impl Fn(Progress) + Send + Sync,
    ) -> Result<Relaunch> {
        let installer = self.installer.clone().ok_or(UpdateError::NotInstallable)?;
        let outcome = self.stage_and_install(&installer, offer, on_progress).await;
        if !matches!(outcome, Err(UpdateError::RollbackFailed { .. })) {
            discard_staging(installer.staging_directory()).await;
        }
        outcome.map(|()| installer.relaunch())
    }

    async fn stage_and_install(
        &self,
        installer: &Arc<dyn Installer>,
        offer: &Offer,
        on_progress: impl Fn(Progress) + Send + Sync,
    ) -> Result<()> {
        let expected = self.feed.checksum_of(offer).await?;
        let staging = installer.staging_directory();
        tokio::fs::create_dir_all(staging)
            .await
            .map_err(io_error("creating the staging directory"))?;
        let destination = staging.join(&offer.asset.name);
        let download = Download {
            asset: &offer.asset,
            expected: &expected,
            destination: &destination,
        };
        let report_percent = |percent| on_progress(Progress::Downloading(percent));
        self.feed.download(&download, &report_percent).await?;
        on_progress(Progress::Installing);
        let installing = Arc::clone(installer);
        let version = offer.version.clone();
        tokio::task::spawn_blocking(move || installing.install(&destination, &version)).await?
    }
}

fn detect_installer() -> Result<Option<Arc<dyn Installer>>> {
    let launch = Launch::from_env().map_err(io_error("locating the running app"))?;
    Ok(installer_for(&launch))
}

async fn discard_staging(staging: &Path) {
    if let Err(error) = tokio::fs::remove_dir_all(staging).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("removing {} failed: {error}", staging.display());
    }
}
