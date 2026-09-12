use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::info;

use super::BirdClient;
use crate::bird::parser::{
    BirdProtocol, BirdRoute, parse_bird_uptime, parse_protocols, parse_routes,
};
use crate::error::AgentError;

pub struct BirdManager<C: BirdClient> {
    client: C,
    config_path: PathBuf,
    bird_binary: PathBuf,
}

impl<C: BirdClient> BirdManager<C> {
    pub fn new(client: C, config_path: &str, bird_binary: &str) -> Self {
        Self {
            client,
            config_path: PathBuf::from(config_path),
            bird_binary: PathBuf::from(bird_binary),
        }
    }

    /// Validate a config file using `bird -p -c <path>`
    pub async fn validate_config(&self, temp_config_path: &Path) -> Result<(), AgentError> {
        let output = Command::new(&self.bird_binary)
            .args(["-p", "-c"])
            .arg(temp_config_path)
            .output()
            .await
            .map_err(|e| {
                AgentError::BirdValidation(format!(
                    "failed to run {}: {e}",
                    self.bird_binary.display()
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AgentError::BirdValidation(format!(
                "bird -p failed (exit {}): {stderr} {stdout}",
                output.status
            )));
        }

        Ok(())
    }

    /// Apply config by sending `configure` to BIRD via socket
    ///
    /// BIRD replies `0003 Reconfigured` when the change applies immediately or
    /// `0004 Reconfiguration in progress` when protocols restart asynchronously.
    /// Both mean the new config was accepted
    pub async fn apply_config(&self) -> Result<(), AgentError> {
        let response = self.client.send_command("configure").await?;

        if response.contains("Reconfigured") || response.contains("Reconfiguration in progress") {
            info!(config_path = %self.config_path.display(), "BIRD config applied");
            Ok(())
        } else {
            Err(AgentError::BirdCommand(format!(
                "configure failed: {response}"
            )))
        }
    }

    /// Get all BGP protocol states
    pub async fn get_protocols(&self) -> Result<Vec<BirdProtocol>, AgentError> {
        let output = self.client.send_command("show protocols all").await?;
        Ok(parse_protocols(&output))
    }

    /// Get the routes a peer is advertising, from `show route protocol <name> all`
    ///
    /// The protocol name goes into a control-socket command, so it is checked
    /// against what BIRD accepts in a symbol: anything else comes from a
    /// tampered config and is refused rather than sent
    pub async fn get_routes(&self, protocol: &str) -> Result<Vec<BirdRoute>, AgentError> {
        if protocol.is_empty()
            || !protocol
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(AgentError::BirdCommand(format!(
                "protocol name is not a BIRD symbol: {protocol:?}"
            )));
        }

        let output = self
            .client
            .send_command(&format!("show route protocol {protocol} all"))
            .await?;
        Ok(parse_routes(&output))
    }

    /// Get BIRD uptime in seconds by parsing `show status` output
    pub async fn get_uptime(&self) -> Option<f64> {
        let output = self.client.send_command("show status").await.ok()?;
        parse_bird_uptime(&output)
    }

    /// Check if BIRD is running
    pub async fn is_running(&self) -> bool {
        self.client.is_running().await
    }

    /// Atomically install a previously-validated config file at `temp_path`
    /// by renaming it over `self.config_path` and fsyncing the parent dir.
    /// Rename is atomic within a single filesystem, so a crash cannot leave
    /// the live config partially written.
    pub async fn commit_config(&self, temp_path: &Path) -> Result<(), AgentError> {
        tokio::fs::rename(temp_path, &self.config_path)
            .await
            .map_err(|e| AgentError::io(&self.config_path, e))?;

        if let Some(parent) = self.config_path.parent() {
            let dir = tokio::fs::File::open(parent)
                .await
                .map_err(|e| AgentError::io(parent, e))?;
            dir.sync_all()
                .await
                .map_err(|e| AgentError::io(parent, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::bird::BirdClient;

    struct MockClient {
        response: Mutex<String>,
        ultimo_comando: Mutex<String>,
    }

    impl MockClient {
        fn with_response(response: &str) -> Self {
            Self {
                response: Mutex::new(response.to_string()),
                ultimo_comando: Mutex::new(String::new()),
            }
        }
    }

    impl BirdClient for MockClient {
        async fn send_command(&self, command: &str) -> Result<String, AgentError> {
            *self.ultimo_comando.lock().unwrap() = command.to_string();
            Ok(self.response.lock().unwrap().clone())
        }

        async fn is_running(&self) -> bool {
            true
        }
    }

    fn manager(response: &str) -> BirdManager<MockClient> {
        BirdManager::new(
            MockClient::with_response(response),
            "/etc/bird/bird.conf",
            "/usr/sbin/bird",
        )
    }

    #[tokio::test]
    async fn apply_config_accepts_immediate_reconfigure() {
        let m = manager("0002-Reading configuration from /etc/bird/bird.conf\n0003 Reconfigured\n");
        assert!(m.apply_config().await.is_ok());
    }

    #[tokio::test]
    async fn apply_config_accepts_reconfiguration_in_progress() {
        let m = manager(
            "0002-Reading configuration from /etc/bird/bird.conf\n0004 Reconfiguration in progress\n",
        );
        assert!(m.apply_config().await.is_ok());
    }

    #[tokio::test]
    async fn apply_config_rejects_errors() {
        let m = manager("8002 /etc/bird/bird.conf, line 5: syntax error\n");
        assert!(m.apply_config().await.is_err());
    }

    #[tokio::test]
    async fn get_routes_pide_las_rutas_de_ese_protocolo() {
        let salida = concat!(
            "1007-Table t_x:\n",
            "1007-192.0.2.0/24         unicast [pb_x 2026-09-12 15:33:06] * (100) [AS1i]\n",
            "1012-\tBGP.as_path: 273973\n",
            "0000 "
        );
        let m = manager(salida);

        let rutas = m.get_routes("pb_APO_45_170_101_11_v4").await.unwrap();

        assert_eq!(rutas.len(), 1);
        assert_eq!(rutas[0].prefix, "192.0.2.0/24");
        assert_eq!(rutas[0].as_path, vec![273973]);
        assert_eq!(
            *m.client.ultimo_comando.lock().unwrap(),
            "show route protocol pb_APO_45_170_101_11_v4 all"
        );
    }

    #[tokio::test]
    async fn get_routes_rechaza_un_nombre_que_no_es_de_bird() {
        // El nombre entra en un comando del socket de control. BIRD solo acepta
        // letras, digitos y guion bajo en un simbolo, asi que cualquier otra
        // cosa viene de un config manipulado y no se manda
        let m = manager("0000 ");

        for malo in ["pb_x\nshow status", "pb x", "pb_x; drop", ""] {
            assert!(
                m.get_routes(malo).await.is_err(),
                "deberia rechazar {malo:?}"
            );
        }
    }
}
