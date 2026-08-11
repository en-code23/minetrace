mod adapter;
mod candidates;
mod registry;
mod validation;

pub use adapter::LauncherAdapter;
pub use registry::AdapterRegistry;
pub use validation::validate_directory;
pub(crate) use validation::validate_directory_with_control;
