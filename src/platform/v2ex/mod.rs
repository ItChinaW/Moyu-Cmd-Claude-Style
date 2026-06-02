use anyhow::Result;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView};

pub async fn list(_http: &HttpClient) -> Result<Vec<ListEntry>> { anyhow::bail!("未实现") }
pub async fn detail(_http: &HttpClient, _token: &str) -> Result<Vec<DetailView>> { anyhow::bail!("未实现") }
