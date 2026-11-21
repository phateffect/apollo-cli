use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    #[serde(rename = "appId")]
    pub app_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_name: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub total: u64,
    pub content: Vec<T>,
    pub page: usize,
    pub size: usize,
}

#[derive(Debug, Deserialize)]
pub struct MultiResponse<T> {
    pub code: u16,
    pub entities: Vec<RichResponse<T>>,
}

#[derive(Debug, Deserialize)]
pub struct RichResponse<T> {
    pub code: u16,
    pub message: Option<Value>,
    pub body: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvClusterInfo {
    pub env: String,
    #[serde(default)]
    pub clusters: Vec<Cluster>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub name: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Namespace {
    #[serde(rename = "baseInfo")]
    pub base_info: NamespaceInfo,
    #[serde(default)]
    pub items: Vec<NamespaceItem>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(rename = "isConfigHidden", default)]
    pub config_hidden: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NamespaceInfo {
    pub id: u64,
    #[serde(rename = "namespaceName")]
    pub namespace_name: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "clusterName")]
    pub cluster_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NamespaceItem {
    pub item: Item,
    #[serde(rename = "isDeleted", default)]
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    #[serde(rename = "namespaceId")]
    pub namespace_id: u64,
    pub key: String,
    pub value: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(rename = "lineNum", default)]
    pub line_num: i64,
    #[serde(rename = "type", default)]
    pub kind: i64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AppOutput<'a> {
    pub app: &'a str,
    pub envs: Vec<AppEnvironment>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AppEnvironment {
    pub env: String,
    pub clusters: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NamespacesOutput<'a> {
    pub app: &'a str,
    pub env: &'a str,
    pub cluster: &'a str,
    pub namespaces: Vec<NamespaceOutput>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NamespaceOutput {
    pub namespace: String,
    pub format: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ContentOutput<'a> {
    pub app: &'a str,
    pub env: &'a str,
    pub cluster: &'a str,
    pub namespace: &'a str,
    pub content: String,
}
