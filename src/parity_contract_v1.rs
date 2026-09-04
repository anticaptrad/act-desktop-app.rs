//! Shared feature-parity contract with `anticaptrad/act-flutter`.
//! Platform-specific behavior belongs only in [`AppPlatformAdapter`].

pub const CROSS_PLATFORM_PARITY_CONTRACT_VERSION: u32 = 1;
pub const FLUTTER_COUNTERPART: &str = "anticaptrad/act-flutter";
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AppSurface { Mobile, FlutterDesktop, RustDesktop }
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AppCapability {
    Authentication, DeepLinks, SecureStorage, Notifications, FileImportExport,
    OfflineCache, BackgroundSync, Telemetry, Accessibility, ApplicationUpdates,
}
pub const REQUIRED_PARITY_CAPABILITIES: &[AppCapability] = &[
    AppCapability::Authentication, AppCapability::DeepLinks,
    AppCapability::SecureStorage, AppCapability::Notifications,
    AppCapability::FileImportExport, AppCapability::OfflineCache,
    AppCapability::BackgroundSync, AppCapability::Telemetry,
    AppCapability::Accessibility, AppCapability::ApplicationUpdates,
];
pub trait AppPlatformAdapter {
    fn surface(&self) -> AppSurface;
    fn supports(&self, capability: AppCapability) -> bool;
}
pub fn verify_required_parity_capabilities(
    adapter: &impl AppPlatformAdapter,
) -> Result<(), Vec<AppCapability>> {
    let missing = REQUIRED_PARITY_CAPABILITIES.iter().copied()
        .filter(|capability| !adapter.supports(*capability)).collect::<Vec<_>>();
    if missing.is_empty() { Ok(()) } else { Err(missing) }
}
