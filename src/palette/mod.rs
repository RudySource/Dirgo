mod coordinator;
pub mod providers;
mod result;
mod session;
mod tui;
mod types;

pub use tui::{PaletteViewOptions, pick};

pub use coordinator::{PaletteCoordinator, PaletteSnapshot, ProviderBatch, ProviderBudget};
pub use result::{PaletteResultFrame, PaletteResultKind};
pub use session::{PaletteSession, PreviewRequest, PreviewResponse};
pub use types::{PaletteAction, PaletteItem, PaletteSource, ProviderState};
