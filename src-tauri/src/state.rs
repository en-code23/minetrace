use std::sync::Arc;

use crate::application::{DashboardService, DiscoveryService, ExplorerService, ScanService};
use crate::storage::Database;

pub struct AppState {
    pub database: Database,
    pub dashboard: DashboardService,
    pub explorer: ExplorerService,
    pub discovery: Arc<DiscoveryService>,
    pub scan: Arc<ScanService>,
}
