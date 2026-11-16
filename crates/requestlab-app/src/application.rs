//! Process-wide resources: the Tokio runtime, the application service and the
//! GPUI event loop.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use requestlab_core::AppService;
use requestlab_ui::ThemeMode;
use tokio::runtime::Runtime;

use crate::window;

/// Everything the desktop process needs before the first frame is drawn.
pub struct RequestLabApplication {
    /// Owns every blocking-free task the UI starts. GPUI has no Tokio reactor,
    /// so all network and database work runs on this runtime.
    runtime: Arc<Runtime>,
    service: AppService,
    theme: ThemeMode,
}

impl RequestLabApplication {
    /// Open local storage and prepare the runtime.
    ///
    /// A missing database file is created; a corrupted one is moved aside and
    /// recreated by `requestlab-storage`, so startup only fails when the data
    /// directory itself is unusable.
    pub fn bootstrap() -> Result<Self> {
        let runtime = Runtime::new().context("could not start the Tokio runtime")?;
        let service = runtime
            .block_on(AppService::open(None))
            .context("could not open the RequestLab database")?;
        let theme = ThemeMode::from_setting(&service.settings().theme);
        tracing::info!(
            database = ?service.storage().path(),
            "RequestLab storage ready"
        );
        Ok(Self {
            runtime: Arc::new(runtime),
            service,
            theme,
        })
    }

    /// Run the GPUI event loop until the last window closes.
    pub fn run(self) -> Result<()> {
        let runtime = self.runtime.clone();
        let service = self.service.clone();
        let theme = self.theme;

        gpui_platform::application()
            .with_assets(ely_gpui_component::Assets)
            .run(move |cx| {
                // Ely loads its fonts and installs the design tokens every Ely
                // component, and the whole palette in `requestlab-ui`, reads.
                // Without it there is no interface to draw, so stop rather than
                // paint a half-initialised window.
                if let Err(error) = ely_gpui_component::init(cx) {
                    tracing::error!(%error, "Ely component library failed to start");
                    eprintln!("RequestLab could not load its interface assets: {error:#}");
                    cx.quit();
                    return;
                }
                requestlab_ui::bind_keys(cx);
                window::open_main_window(cx, service, runtime, theme);
                cx.activate(true);
            });

        Ok(())
    }
}
