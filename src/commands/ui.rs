use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};
use aegis_tui::{AegisClient, AppState, Tui};

pub async fn run(
    _printer: &Printer,
    daemon_client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let client = AegisClient::new(
        daemon_client.uds_path().to_path_buf(),
        anchor.project_root.clone(),
    );
    let app = AppState::new(anchor.project_root.clone());

    let mut tui = Tui::new(app, client).map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    tui.run()
        .await
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    Ok(())
}
