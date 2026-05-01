use crate::messaging::{MessageDeliveryReceipt, MessageRouter};
use crate::registry::FileRegistry;
use crate::registry::LockedFile;
use crate::storage::ProjectStorage;
use aegis_core::{AegisError, AgentRegistry, AgentStatus, MessageType, Result, StorageBackend};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};
use uuid::Uuid;

fn default_store_version() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ClarificationStatus {
    Open,
    Answered,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ClarifierSource {
    HumanCli,
    HumanTui,
    Telegram,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ClarificationResponse {
    pub request_id: Uuid,
    pub answer: String,
    pub payload: serde_json::Value,
    pub answered_by: ClarifierSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ClarificationRequest {
    pub request_id: Uuid,
    pub agent_id: Uuid,
    pub task_id: Option<Uuid>,
    pub question: String,
    pub context: serde_json::Value,
    pub priority: i32,
    pub status: ClarificationStatus,
    pub created_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub delivered_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub delivery_error: Option<String>,
    #[serde(default)]
    pub response: Option<ClarificationResponse>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ClarificationStore {
    #[serde(default = "default_store_version")]
    pub version: u32,
    #[serde(default)]
    pub requests: Vec<ClarificationRequest>,
}

#[derive(Clone)]
pub struct ClarificationService {
    registry: Arc<FileRegistry>,
    storage: Arc<ProjectStorage>,
    message_router: Arc<MessageRouter>,
}

impl ClarificationService {
    pub fn new(
        registry: Arc<FileRegistry>,
        storage: Arc<ProjectStorage>,
        message_router: Arc<MessageRouter>,
    ) -> Self {
        Self {
            registry,
            storage,
            message_router,
        }
    }

    pub fn request(
        &self,
        agent_raw: &str,
        task_id: Option<Uuid>,
        question: &str,
        context: serde_json::Value,
        priority: i32,
    ) -> Result<ClarificationRequest> {
        let agent_id = self.resolve_agent_id(agent_raw)?;
        let request = ClarificationRequest {
            request_id: Uuid::new_v4(),
            agent_id,
            task_id,
            question: question.to_string(),
            context,
            priority,
            status: ClarificationStatus::Open,
            created_at: Utc::now(),
            answered_at: None,
            delivered_at: None,
            delivery_error: None,
            response: None,
        };

        let mut store = self.load_store()?;
        store.requests.push(request.clone());
        self.save_store(&store)?;
        self.write_request(&request)?;
        self.write_human_inbox(&request)?;
        self.write_handoff_snapshot(&request)?;

        if let Ok(Some(agent)) = AgentRegistry::get(self.registry.as_ref(), agent_id) {
            if matches!(
                agent.status,
                AgentStatus::Active | AgentStatus::Starting | AgentStatus::Reporting
            ) {
                AgentRegistry::update_status(
                    self.registry.as_ref(),
                    agent_id,
                    AgentStatus::Paused,
                )?;
            }
        }

        Ok(request)
    }

    pub fn list(&self) -> Result<Vec<ClarificationRequest>> {
        let mut requests = self.load_store()?.requests;
        requests.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.request_id.cmp(&b.request_id))
        });
        Ok(requests)
    }

    pub fn list_for_agent(&self, agent_raw: &str) -> Result<Vec<ClarificationRequest>> {
        let agent_id = self.resolve_agent_id(agent_raw)?;
        Ok(self
            .list()?
            .into_iter()
            .filter(|request| request.agent_id == agent_id)
            .collect())
    }

    pub fn show(&self, request_id: Uuid) -> Result<ClarificationRequest> {
        self.list()?
            .into_iter()
            .find(|request| request.request_id == request_id)
            .ok_or_else(|| AegisError::IpcProtocol {
                reason: format!("Unknown clarification request `{request_id}`"),
            })
    }

    pub fn latest_for_agent(&self, agent_raw: &str) -> Result<Option<ClarificationRequest>> {
        let agent_id = self.resolve_agent_id(agent_raw)?;
        let requests = self.list()?;
        if let Some(open) = requests.iter().find(|request| {
            request.agent_id == agent_id && request.status == ClarificationStatus::Open
        }) {
            return Ok(Some(open.clone()));
        }

        Ok(requests
            .into_iter()
            .find(|request| request.agent_id == agent_id))
    }

    pub async fn answer(
        &self,
        request_id: Uuid,
        answer: &str,
        payload: serde_json::Value,
        answered_by: ClarifierSource,
    ) -> Result<ClarificationRequest> {
        let mut store = self.load_store()?;
        let response = ClarificationResponse {
            request_id,
            answer: answer.to_string(),
            payload,
            answered_by,
            created_at: Utc::now(),
        };

        let mut snapshot = {
            let request = store
                .requests
                .iter_mut()
                .find(|request| request.request_id == request_id)
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: format!("Unknown clarification request `{request_id}`"),
                })?;

            request.status = ClarificationStatus::Answered;
            request.answered_at = Some(response.created_at);
            request.response = Some(response.clone());
            request.delivery_error = None;
            request.clone()
        };

        self.save_store(&store)?;
        self.write_human_inbox(&snapshot)?;

        let mut warning = None;
        match self.deliver_response(&snapshot).await {
            Ok(_) => {
                snapshot.delivered_at = Some(Utc::now());
                snapshot.delivery_error = None;
            }
            Err(err) => {
                warning = Some(err.to_string());
                snapshot.delivery_error = warning.clone();
            }
        }

        {
            let request = store
                .requests
                .iter_mut()
                .find(|request| request.request_id == request_id)
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: format!("Unknown clarification request `{request_id}`"),
                })?;
            *request = snapshot.clone();
            if warning.is_none() {
                request.delivered_at = snapshot.delivered_at;
            }
        }

        self.save_store(&store)?;
        self.write_human_inbox(&snapshot)?;

        Ok(snapshot)
    }

    pub async fn wait(
        &self,
        request_or_agent_raw: &str,
        timeout: Option<Duration>,
    ) -> Result<ClarificationRequest> {
        let deadline = timeout.map(|duration| Instant::now() + duration);
        let request_id = self.resolve_wait_request_id(request_or_agent_raw)?;

        loop {
            let request = self.show(request_id)?;
            if !matches!(request.status, ClarificationStatus::Open) {
                return Ok(request);
            }

            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(AegisError::IpcProtocol {
                        reason: format!("Clarification wait timed out for `{request_id}`"),
                    });
                }
            }

            sleep(Duration::from_millis(250)).await;
        }
    }

    pub async fn recover_pending_deliveries(&self) -> Result<usize> {
        let mut store = self.load_store()?;
        let mut delivered = 0usize;

        for request in &mut store.requests {
            if request.status != ClarificationStatus::Answered {
                continue;
            }
            if request.response.is_none() || request.delivered_at.is_some() {
                continue;
            }

            if self.deliver_response(request).await.is_ok() {
                let now = Utc::now();
                request.delivered_at = Some(now);
                request.delivery_error = None;
                delivered += 1;
            }
        }

        self.save_store(&store)?;
        Ok(delivered)
    }

    pub fn resolve_request_id(&self, raw: &str) -> Result<Uuid> {
        let store = self.load_store()?;

        if let Ok(uuid) = Uuid::parse_str(raw) {
            if store.requests.iter().any(|r| r.request_id == uuid) {
                return Ok(uuid);
            }
        }

        let matches: Vec<Uuid> = store
            .requests
            .iter()
            .filter(|r| r.request_id.to_string().starts_with(raw))
            .map(|r| r.request_id)
            .collect();

        match matches.as_slice() {
            [id] => Ok(*id),
            [] => {
                // Try agent resolution as a fallback - if it's an agent ID or prefix, get its latest open request
                if let Ok(agent_id) = self.resolve_agent_id(raw) {
                    if let Some(req) = self.latest_for_agent(&agent_id.to_string())? {
                        return Ok(req.request_id);
                    }
                }
                Err(AegisError::IpcProtocol {
                    reason: format!("Unknown clarification request ID or prefix `{raw}`"),
                })
            }
            _ => Err(AegisError::IpcProtocol {
                reason: format!("Ambiguous clarification request ID prefix `{raw}`"),
            }),
        }
    }

    fn resolve_agent_id(&self, raw: &str) -> Result<Uuid> {
        let agents = AgentRegistry::list_all(self.registry.as_ref())?;

        if let Ok(uuid) = Uuid::parse_str(raw) {
            if agents.iter().any(|agent| agent.agent_id == uuid) {
                return Ok(uuid);
            }
            return Err(AegisError::AgentNotFound { agent_id: uuid });
        }

        let matches: Vec<Uuid> = agents
            .iter()
            .filter(|agent| agent.agent_id.to_string().starts_with(raw))
            .map(|agent| agent.agent_id)
            .collect();

        match matches.as_slice() {
            [agent_id] => Ok(*agent_id),
            [] => Err(AegisError::IpcProtocol {
                reason: format!("Unknown agent_id prefix `{raw}`"),
            }),
            _ => Err(AegisError::IpcProtocol {
                reason: format!("Ambiguous agent_id prefix `{raw}`"),
            }),
        }
    }

    fn resolve_wait_request_id(&self, raw: &str) -> Result<Uuid> {
        self.resolve_request_id(raw)
    }

    fn load_store(&self) -> Result<ClarificationStore> {
        let path = self.storage.clarifications_path();
        if !path.exists() {
            return Ok(ClarificationStore::default());
        }

        let content = fs::read_to_string(&path).map_err(|source| AegisError::StorageIo {
            path: path.clone(),
            source,
        })?;
        if content.trim().is_empty() {
            return Ok(ClarificationStore::default());
        }

        serde_json::from_str(&content)
            .map_err(|source| AegisError::RegistryCorrupted { path, source })
    }

    fn save_store(&self, store: &ClarificationStore) -> Result<()> {
        let path = self.storage.clarifications_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AegisError::StorageIo {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let mut file = LockedFile::open_exclusive(&path)?;
        file.write_json_atomic(store)
    }

    fn write_request(&self, request: &ClarificationRequest) -> Result<()> {
        self.write_json_atomic(
            &self.storage.clarification_inbox_path(request.request_id),
            request,
        )
    }

    fn write_human_inbox(&self, request: &ClarificationRequest) -> Result<()> {
        self.write_request(request)
    }

    fn write_handoff_snapshot(&self, request: &ClarificationRequest) -> Result<()> {
        if let Some(task_id) = request.task_id {
            self.write_json_atomic(
                &self
                    .storage
                    .clarification_handoff_path(task_id, request.request_id),
                request,
            )?;
        }
        Ok(())
    }

    async fn deliver_response(
        &self,
        request: &ClarificationRequest,
    ) -> Result<MessageDeliveryReceipt> {
        let response = request
            .response
            .as_ref()
            .ok_or_else(|| AegisError::IpcProtocol {
                reason: format!(
                    "Clarification request `{}` has no response",
                    request.request_id
                ),
            })?;

        let payload = serde_json::json!({
            "type": "clarification_response",
            "request_id": request.request_id,
            "agent_id": request.agent_id,
            "task_id": request.task_id,
            "question": request.question,
            "context": request.context,
            "answer": response.answer.clone(),
            "payload": response.payload.clone(),
            "answered_by": response.answered_by.clone(),
            "answered_at": response.created_at,
        });

        self.message_router
            .send(
                None,
                &request.agent_id.to_string(),
                MessageType::Handoff,
                payload,
            )
            .await
    }

    fn write_json_atomic<T: Serialize>(&self, path: &std::path::Path, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AegisError::StorageIo {
                path: parent.to_path_buf(),
                source,
            })?;
            let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|source| {
                AegisError::StorageIo {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
            serde_json::to_writer_pretty(&mut tmp, value).map_err(|source| {
                AegisError::RegistryCorrupted {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
            tmp.persist(path).map_err(|e| AegisError::StorageIo {
                path: path.to_path_buf(),
                source: e.error,
            })?;
            return Ok(());
        }

        Err(AegisError::StorageIo {
            path: path.to_path_buf(),
            source: std::io::Error::other("missing parent"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::MessageRouter;
    use crate::registry::FileRegistry;
    use crate::storage::ProjectStorage;
    use aegis_core::{Agent, AgentKind, AgentRegistry, AgentStatus};
    use tempfile::tempdir;

    fn write_minimal_config(project_root: &std::path::Path) {
        let config = r#"
[providers.claude-code]
binary = "claude-code"

[splinter_defaults]
cli_provider = "claude-code"
"#;
        std::fs::write(project_root.join("aegis.toml"), config).unwrap();
    }

    fn test_agent(agent_id: Uuid, status: AgentStatus) -> Agent {
        let now = Utc::now();
        Agent {
            agent_id,
            name: "agent-one".to_string(),
            kind: AgentKind::Splinter,
            status,
            role: "splinter".to_string(),
            parent_id: None,
            task_id: None,
            tmux_session: "aegis".to_string(),
            tmux_window: 0,
            tmux_pane: "%0".to_string(),
            worktree_path: "/tmp/worktree".into(),
            cli_provider: "claude-code".to_string(),
            fallback_cascade: vec!["codex".to_string()],
            sandbox_profile: "/tmp/profile.sb".into(),
            log_path: "/tmp/log.log".into(),
            created_at: now,
            updated_at: now,
            terminated_at: None,
        }
    }

    fn service(project_root: &std::path::Path) -> (ClarificationService, Uuid, Arc<FileRegistry>) {
        write_minimal_config(project_root);
        let storage = Arc::new(ProjectStorage::new(project_root.to_path_buf()));
        storage.ensure_layout().unwrap();
        FileRegistry::init(storage.as_ref()).unwrap();
        let registry = Arc::new(FileRegistry::new(storage.clone()));
        let agent_id = Uuid::parse_str("603685e0-1111-2222-3333-444444444444").unwrap();
        AgentRegistry::insert(
            registry.as_ref(),
            &test_agent(agent_id, AgentStatus::Active),
        )
        .unwrap();

        let router = Arc::new(MessageRouter::new(registry.clone(), storage.clone(), None));
        (
            ClarificationService::new(registry.clone(), storage, router),
            agent_id,
            registry,
        )
    }

    #[test]
    fn request_writes_store_and_pauses_agent() {
        let dir = tempdir().unwrap();
        let (service, agent_id, registry) = service(dir.path());

        let request = service
            .request(
                &agent_id.to_string(),
                Some(Uuid::new_v4()),
                "Need confirmation",
                serde_json::json!({"task": "t1"}),
                10,
            )
            .unwrap();

        assert_eq!(request.agent_id, agent_id);
        assert_eq!(request.status, ClarificationStatus::Open);
        assert!(service.storage.clarifications_path().exists());
        assert!(service
            .storage
            .clarification_inbox_path(request.request_id)
            .exists());

        let agent = AgentRegistry::get(registry.as_ref(), agent_id)
            .unwrap()
            .unwrap();
        assert_eq!(agent.status, AgentStatus::Paused);
    }

    #[tokio::test]
    async fn answer_updates_store_and_delivers_message() {
        let dir = tempdir().unwrap();
        let (service, agent_id, _registry) = service(dir.path());

        let request = service
            .request(
                &agent_id.to_string(),
                None,
                "Need confirmation",
                serde_json::json!({}),
                5,
            )
            .unwrap();

        let answered = service
            .answer(
                request.request_id,
                "Use the default path",
                serde_json::json!({"choice": "default"}),
                ClarifierSource::HumanCli,
            )
            .await
            .unwrap();

        assert_eq!(answered.status, ClarificationStatus::Answered);
        assert!(answered.response.is_some());

        let store = service.load_store().unwrap();
        let stored = store
            .requests
            .iter()
            .find(|candidate| candidate.request_id == request.request_id)
            .unwrap();
        assert_eq!(stored.status, ClarificationStatus::Answered);
        assert_eq!(
            stored.response.as_ref().unwrap().answer,
            "Use the default path"
        );
    }

    #[tokio::test]
    async fn wait_resolves_agent_prefix() {
        let dir = tempdir().unwrap();
        let (service, agent_id, _registry) = service(dir.path());

        let request = service
            .request(
                &agent_id.to_string(),
                None,
                "Need confirmation",
                serde_json::json!({}),
                5,
            )
            .unwrap();

        let answered = service
            .answer(
                request.request_id,
                "Proceed",
                serde_json::json!({}),
                ClarifierSource::HumanCli,
            )
            .await
            .unwrap();

        let waited = service
            .wait(&agent_id.to_string(), Some(Duration::from_secs(2)))
            .await
            .unwrap();
        assert_eq!(waited.request_id, answered.request_id);
    }

    #[tokio::test]
    async fn recovery_resends_undelivered_answers() {
        let dir = tempdir().unwrap();
        let (service, agent_id, _registry) = service(dir.path());

        let request = service
            .request(
                &agent_id.to_string(),
                None,
                "Need confirmation",
                serde_json::json!({}),
                5,
            )
            .unwrap();

        let answered = service
            .answer(
                request.request_id,
                "Proceed",
                serde_json::json!({}),
                ClarifierSource::HumanCli,
            )
            .await
            .unwrap();

        let mut store = service.load_store().unwrap();
        let entry = store
            .requests
            .iter_mut()
            .find(|candidate| candidate.request_id == answered.request_id)
            .unwrap();
        entry.delivered_at = None;
        entry.delivery_error = Some("pending".to_string());
        service.save_store(&store).unwrap();

        let delivered = service.recover_pending_deliveries().await.unwrap();
        assert_eq!(delivered, 1);

        let store = service.load_store().unwrap();
        let entry = store
            .requests
            .iter()
            .find(|candidate| candidate.request_id == answered.request_id)
            .unwrap();
        assert!(entry.delivered_at.is_some());
        assert!(entry.delivery_error.is_none());
    }
}
