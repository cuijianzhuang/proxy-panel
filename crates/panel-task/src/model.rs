use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    ApplyConfig,
    Restart,
    CheckHealth,
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApplyConfig => "apply_config",
            Self::Restart => "restart",
            Self::CheckHealth => "check_health",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "apply_config" => Some(Self::ApplyConfig),
            "restart" => Some(Self::Restart),
            "check_health" => Some(Self::CheckHealth),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Failed,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id:          i64,
    pub node_id:     i64,
    pub kind:        TaskKind,
    pub status:      TaskStatus,
    pub payload:     serde_json::Value,
    pub log:         String,
    pub error:       Option<String>,
    pub started_at:  Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewTask {
    pub node_id: i64,
    pub kind:    TaskKind,
    pub payload: serde_json::Value,
}
