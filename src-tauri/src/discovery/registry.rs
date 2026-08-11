use std::sync::Arc;

use super::{
    LauncherAdapter,
    adapter::{OfficialAdapter, PrismAdapter},
};

#[derive(Clone)]
pub struct AdapterRegistry {
    adapters: Arc<Vec<Arc<dyn LauncherAdapter>>>,
}

impl AdapterRegistry {
    pub fn standard() -> Self {
        Self {
            adapters: Arc::new(vec![Arc::new(OfficialAdapter), Arc::new(PrismAdapter)]),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn LauncherAdapter>> {
        self.adapters.iter()
    }
}
