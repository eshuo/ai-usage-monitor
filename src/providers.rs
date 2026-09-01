/**
 * Provider 模块 — 各 AI 厂商用量查询逻辑
 *
 * 参考 CC Switch coding_plan.rs 的 API 端点和解析逻辑
 */

use reqwest::{Client, header::HeaderMap, header::HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

// ── 数据类型 ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaTier {
    pub name: String,
    pub label: String,
    pub used_percentage: f64,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub resets_at: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceInfo {
    pub available: f64,
    pub voucher: f64,
    pub cash: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageResult {
    pub config_id: String,
    pub provider_id: String,
    pub success: bool,
    pub tiers: Vec<QuotaTier>,
    pub balance: Option<BalanceInfo>,
    pub level: Option<String>,
    pub error: Option<String>,
    pub queried_at: i64,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
    pub placeholder: Option<String>,
    pub default: Option<String>,
    pub options: Option<Vec<FieldOption>>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub auth_type: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderGroup {
    pub id: String,
    pub label: String,
    pub providers: Vec<ProviderInfo>,
}

// ── 工具函数 ──────────────────────────────────────────────

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn parse_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn to_iso8601(v: &Value) -> Option<String> {
    if v.is_null() { return None; }
    if let Some(s) = v.as_str() {
        if s.is_empty() { return None; }
        return Some(s.to_string());
    }
    if let Some(n) = parse_number(v) {
        if n <= 0.0 { return None; }
        let ms = if n < 1e12 { n * 1000.0 } else { n };
        let secs = (ms / 1000.0) as i64;
        let nanos = ((ms % 1000.0).abs() * 1e6) as u32;
        if let Some(dt) = chrono::DateTime::from_timestamp(secs, nanos) {
            return Some(dt.to_rfc3339());
        }
    }
    None
}

fn build_headers(auth_type: &str, api_key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("Accept", HeaderValue::from_static("application/json"));
    if auth_type == "bearer" && !api_key.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {}", api_key)) {
            headers.insert("Authorization", v);
        }
    } else if auth_type == "raw" && !api_key.is_empty() {
        if let Ok(v) = HeaderValue::from_str(api_key) {
            headers.insert("Authorization", v);
        }
    }
    headers
}

fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| Client::new())
}

async fn get_json(client: &Client, url: &str, headers: HeaderMap) -> Result<(u16, Option<Value>, String), String> {
    let resp = client.get(url).headers(headers).send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let raw = resp.text().await.map_err(|e| e.to_string())?;
    let data = serde_json::from_str(&raw).ok();
    Ok((status, data, raw))
}

fn error_result(config_id: &str, provider_id: &str, error: &str) -> UsageResult {
    UsageResult {
        config_id: config_id.into(),
        provider_id: provider_id.into(),
        success: false,
        tiers: vec![],
        balance: None,
        level: None,
        error: Some(error.into()),
        queried_at: now_ms(),
        http_status: None,
    }
}

fn raw_error(error: &str) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    (vec![], None, None, Some(error.into()), None)
}

// ── Kimi Coding Plan ──────────────────────────────────────

async fn query_kimi_coding(creds: &HashMap<String, String>) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    let base_url = creds.get("baseUrl").map(|s| s.trim_end_matches('/')).unwrap_or("https://api.kimi.com");
    let url = format!("{}/coding/v1/usages", base_url);
    let api_key = creds.get("apiKey").map(|s| s.as_str()).unwrap_or("");
    let headers = build_headers("bearer", api_key);
    let client = http_client();

    match get_json(&client, &url, headers).await {
        Ok((status, data, raw)) => {
            if status == 401 || status == 403 {
                return raw_error(&format!("认证失败 (HTTP {})：API Key 无效", status));
            }
            if status != 200 || data.is_none() {
                let snippet = &raw[..raw.len().min(200)];
                return raw_error(&format!("API错误 (HTTP {}): {}", status, snippet));
            }
            let data = data.unwrap();
            let mut tiers = vec![];

            // limits[] → 5小时窗口
            if let Some(limits) = data.get("limits").and_then(|v| v.as_array()) {
                for item in limits {
                    let detail = item.get("detail").unwrap_or(item);
                    let limit = parse_number(&detail.get("limit").unwrap_or(&Value::Null));
                    let remaining = parse_number(&detail.get("remaining").unwrap_or(&Value::Null)).unwrap_or(0.0);
                    let resets_at = to_iso8601(&detail.get("resetTime").unwrap_or(&Value::Null));
                    if let Some(limit) = limit {
                        let used = (limit - remaining).max(0.0);
                        let pct = if limit > 0.0 { (used / limit) * 100.0 } else { 0.0 };
                        tiers.push(QuotaTier {
                            name: "five_hour".into(),
                            label: "5小时窗口".into(),
                            used_percentage: pct.min(100.0),
                            used: Some(used),
                            limit: Some(limit),
                            remaining: Some(remaining),
                            resets_at,
                            unit: Some("tokens".into()),
                        });
                    }
                }
            }

            // usage → 周限额
            if let Some(usage) = data.get("usage") {
                let limit = parse_number(&usage.get("limit").unwrap_or(&Value::Null));
                let remaining = parse_number(&usage.get("remaining").unwrap_or(&Value::Null)).unwrap_or(0.0);
                let resets_at = to_iso8601(&usage.get("resetTime").unwrap_or(&Value::Null));
                if let Some(limit) = limit {
                    let used = (limit - remaining).max(0.0);
                    let pct = if limit > 0.0 { (used / limit) * 100.0 } else { 0.0 };
                    tiers.push(QuotaTier {
                        name: "weekly".into(),
                        label: "每周限额".into(),
                        used_percentage: pct.min(100.0),
                        used: Some(used),
                        limit: Some(limit),
                        remaining: Some(remaining),
                        resets_at,
                        unit: Some("tokens".into()),
                    });
                }
            }

            (tiers, None, None, None, Some(status))
        }
        Err(e) => raw_error(&format!("网络错误: {}", e)),
    }
}

// ── Kimi Balance ──────────────────────────────────────────

async fn query_kimi_balance(creds: &HashMap<String, String>) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    let base_url = creds.get("baseUrl").map(|s| s.trim_end_matches('/')).unwrap_or("https://api.moonshot.cn");
    let url = format!("{}/v1/users/me/balance", base_url);
    let api_key = creds.get("apiKey").map(|s| s.as_str()).unwrap_or("");
    let headers = build_headers("bearer", api_key);
    let client = http_client();

    match get_json(&client, &url, headers).await {
        Ok((status, data, raw)) => {
            if status == 401 || status == 403 {
                return raw_error(&format!("认证失败 (HTTP {})：API Key 无效", status));
            }
            if status != 200 || data.is_none() {
                let snippet = &raw[..raw.len().min(200)];
                return raw_error(&format!("API错误 (HTTP {}): {}", status, snippet));
            }
            let data = data.unwrap();
            let bd = data.get("data").unwrap_or(&data);

            let balance = BalanceInfo {
                available: parse_number(&bd.get("available_balance").unwrap_or(&Value::Null)).unwrap_or(0.0),
                voucher: parse_number(&bd.get("voucher_balance").unwrap_or(&Value::Null)).unwrap_or(0.0),
                cash: parse_number(&bd.get("cash_balance").unwrap_or(&Value::Null)).unwrap_or(0.0),
                currency: "CNY".into(),
            };
            (vec![], Some(balance), None, None, Some(status))
        }
        Err(e) => raw_error(&format!("网络错误: {}", e)),
    }
}

// ── 智谱 GLM ──────────────────────────────────────────────

/// 智谱响应里单条限额条目
#[derive(Default)]
struct ZhipuEntry {
    percentage: f64,
    resets_at: Option<String>,
    used: Option<f64>,
    limit: Option<f64>,
    remaining: Option<f64>,
}

/// 把智谱 `data.limits[]` 解析为 tier 列表
///
/// 新套餐 (2026-02 后) 返回 `CREDIT_LIMIT` 类型并带 `usage`/`currentValue`/
/// `remaining` 数值字段，老套餐为 `TOKENS_LIMIT`；`unit: 3` 为 5 小时窗口，
/// `unit: 6` 为每周窗口。
fn parse_zhipu_tiers(data: &Value) -> Vec<QuotaTier> {
    fn to_tier(name: &str, label: &str, e: ZhipuEntry) -> QuotaTier {
        QuotaTier {
            name: name.into(),
            label: label.into(),
            used_percentage: e.percentage.min(100.0),
            used: e.used,
            limit: e.limit,
            remaining: e.remaining,
            resets_at: e.resets_at,
            unit: Some(if e.used.is_some() && e.limit.is_some() { "积分".into() } else { "%".into() }),
        }
    }

    let limits = data.get("limits").and_then(|v| v.as_array())
        .map(|a| a.as_slice()).unwrap_or(&[]);

    let mut five_hour: Option<ZhipuEntry> = None;
    let mut weekly: Option<ZhipuEntry> = None;
    let mut unclassified: Vec<(Option<i64>, ZhipuEntry)> = vec![];

    for item in limits {
        let limit_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !(limit_type.eq_ignore_ascii_case("TOKENS_LIMIT")
            || limit_type.eq_ignore_ascii_case("CREDIT_LIMIT"))
        {
            continue;
        }

        let reset_ms = item.get("nextResetTime").and_then(|v| v.as_i64());
        let entry = ZhipuEntry {
            percentage: parse_number(&item.get("percentage").unwrap_or(&Value::Null)).unwrap_or(0.0),
            resets_at: to_iso8601(&item.get("nextResetTime").unwrap_or(&Value::Null)),
            used: parse_number(&item.get("currentValue").unwrap_or(&Value::Null)),
            limit: parse_number(&item.get("usage").unwrap_or(&Value::Null)),
            remaining: parse_number(&item.get("remaining").unwrap_or(&Value::Null)),
        };

        let unit = item.get("unit").and_then(|v| v.as_i64()).unwrap_or(0);
        match unit {
            3 if five_hour.is_none() => five_hour = Some(entry),
            6 if weekly.is_none() => weekly = Some(entry),
            _ => unclassified.push((reset_ms, entry)),
        }
    }

    // 兜底启发式: 无重置时间的条目优先归 5 小时窗口，其余按重置时间升序
    unclassified.sort_by_key(|(reset, _)| (reset.is_some(), reset.unwrap_or(0)));
    for (_, entry) in unclassified {
        if five_hour.is_none() { five_hour = Some(entry); }
        else if weekly.is_none() { weekly = Some(entry); }
    }

    let mut tiers = vec![];
    if let Some(e) = five_hour {
        tiers.push(to_tier("five_hour", "5小时窗口", e));
    }
    if let Some(e) = weekly {
        tiers.push(to_tier("weekly", "每周限额", e));
    }
    tiers
}

async fn query_zhipu(creds: &HashMap<String, String>) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    let base_url = creds.get("baseUrl").map(|s| s.trim_end_matches('/')).unwrap_or("https://open.bigmodel.cn");
    let url = format!("{}/api/monitor/usage/quota/limit", base_url);
    let api_key = creds.get("apiKey").map(|s| s.as_str()).unwrap_or("");
    let mut headers = build_headers("raw", api_key); // 智谱不加 Bearer
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en"));
    let client = http_client();

    match get_json(&client, &url, headers).await {
        Ok((status, data, raw)) => {
            if status == 401 || status == 403 {
                return raw_error(&format!("认证失败 (HTTP {})：API Key 无效", status));
            }
            if status != 200 || data.is_none() {
                let snippet = &raw[..raw.len().min(200)];
                return raw_error(&format!("API错误 (HTTP {}): {}", status, snippet));
            }
            let data = data.unwrap();

            // 检查业务级别错误
            if data.get("success").and_then(|v| v.as_bool()) == Some(false) {
                let msg = data.get("msg").or_else(|| data.get("message"))
                    .and_then(|v| v.as_str()).unwrap_or("未知错误");
                return raw_error(&format!("API业务错误: {}", msg));
            }

            let resp_data = match data.get("data") {
                Some(d) => d,
                None => return raw_error("响应缺少 'data' 字段"),
            };

            let tiers = parse_zhipu_tiers(resp_data);

            let level = resp_data.get("level").and_then(|v| v.as_str()).map(|s| s.to_string());
            (tiers, None, level, None, Some(status))
        }
        Err(e) => raw_error(&format!("网络错误: {}", e)),
    }
}

// ── MiniMax ───────────────────────────────────────────────

async fn query_minimax(creds: &HashMap<String, String>) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    let base_url = creds.get("baseUrl").map(|s| s.trim_end_matches('/')).unwrap_or("https://api.minimaxi.com");
    let url = format!("{}/v1/api/openplatform/coding_plan/remains", base_url);
    let api_key = creds.get("apiKey").map(|s| s.as_str()).unwrap_or("");
    let headers = build_headers("bearer", api_key);
    let client = http_client();

    match get_json(&client, &url, headers).await {
        Ok((status, data, raw)) => {
            if status == 401 || status == 403 {
                return raw_error(&format!("认证失败 (HTTP {})：API Key 无效", status));
            }
            if status != 200 || data.is_none() {
                let snippet = &raw[..raw.len().min(200)];
                return raw_error(&format!("API错误 (HTTP {}): {}", status, snippet));
            }
            let data = data.unwrap();

            // 检查业务级别错误
            if let Some(base_resp) = data.get("base_resp") {
                let status_code = base_resp.get("status_code").and_then(|v| v.as_i64()).unwrap_or(0);
                if status_code != 0 {
                    let msg = base_resp.get("status_msg").and_then(|v| v.as_str()).unwrap_or("未知错误");
                    return raw_error(&format!("API业务错误 (code {}): {}", status_code, msg));
                }
            }

            let model_remains = data.get("model_remains").and_then(|v| v.as_array())
                .map(|a| a.as_slice()).unwrap_or(&[]);

            let item = model_remains.iter().find(|m| {
                m.get("model_name").and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase() == "general").unwrap_or(false)
            });

            let mut tiers = vec![];
            if let Some(item) = item {
                // 5h 桶
                if let Some(remain5h) = parse_number(&item.get("current_interval_remaining_percent").unwrap_or(&Value::Null)) {
                    let used_pct = (100.0 - remain5h).max(0.0).min(100.0);
                    tiers.push(QuotaTier {
                        name: "five_hour".into(), label: "5小时窗口".into(),
                        used_percentage: used_pct,
                        used: None, limit: None, remaining: None,
                        resets_at: to_iso8601(&item.get("end_time").unwrap_or(&Value::Null)),
                        unit: Some("%".into()),
                    });
                }

                // 周桶: 仅当 status==1 时激活
                if item.get("current_weekly_status").and_then(|v| v.as_i64()) == Some(1) {
                    if let Some(remain_w) = parse_number(&item.get("current_weekly_remaining_percent").unwrap_or(&Value::Null)) {
                        let used_pct = (100.0 - remain_w).max(0.0).min(100.0);
                        tiers.push(QuotaTier {
                            name: "weekly".into(), label: "每周限额".into(),
                            used_percentage: used_pct,
                            used: None, limit: None, remaining: None,
                            resets_at: to_iso8601(&item.get("weekly_end_time").unwrap_or(&Value::Null)),
                            unit: Some("%".into()),
                        });
                    }
                }
            }

            (tiers, None, None, None, Some(status))
        }
        Err(e) => raw_error(&format!("网络错误: {}", e)),
    }
}

// ── DeepSeek ──────────────────────────────────────────────

async fn query_deepseek(creds: &HashMap<String, String>) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    let base_url = creds.get("baseUrl").map(|s| s.trim_end_matches('/')).unwrap_or("https://api.deepseek.com");
    let url = format!("{}/user/balance", base_url);
    let api_key = creds.get("apiKey").map(|s| s.as_str()).unwrap_or("");
    let headers = build_headers("bearer", api_key);
    let client = http_client();

    match get_json(&client, &url, headers).await {
        Ok((status, data, raw)) => {
            if status == 401 || status == 403 {
                return raw_error(&format!("认证失败 (HTTP {})：API Key 无效", status));
            }
            if status != 200 || data.is_none() {
                let snippet = &raw[..raw.len().min(200)];
                return raw_error(&format!("API错误 (HTTP {}): {}", status, snippet));
            }
            let data = data.unwrap();

            let info = data.get("balance_infos").and_then(|v| v.as_array())
                .and_then(|a| a.first()).unwrap_or(&Value::Null);

            let balance = BalanceInfo {
                available: parse_number(&info.get("total_balance").unwrap_or(&Value::Null)).unwrap_or(0.0),
                voucher: parse_number(&info.get("granted_balance").unwrap_or(&Value::Null)).unwrap_or(0.0),
                cash: parse_number(&info.get("topped_up_balance").unwrap_or(&Value::Null)).unwrap_or(0.0),
                currency: info.get("currency").and_then(|v| v.as_str()).unwrap_or("CNY").into(),
            };
            (vec![], Some(balance), None, None, Some(status))
        }
        Err(e) => raw_error(&format!("网络错误: {}", e)),
    }
}

// ── Custom ────────────────────────────────────────────────

fn get_path<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = v;
    for part in path.split('.') {
        if let Some(bracket_start) = part.find('[') {
            let key = &part[..bracket_start];
            if !key.is_empty() {
                current = current.get(key)?;
            }
            let idx_str = &part[bracket_start + 1..part.len() - 1];
            let idx: usize = idx_str.parse().ok()?;
            current = current.get(idx)?;
        } else {
            current = current.get(part)?;
        }
    }
    Some(current)
}

async fn query_custom(creds: &HashMap<String, String>) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    let base_url = creds.get("baseUrl").map(|s| s.trim_end_matches('/')).unwrap_or("");
    let path = creds.get("path").map(|s| {
        let p = s.trim();
        if p.starts_with('/') { p.to_string() } else { format!("/{}", p) }
    }).unwrap_or_default();
    let url = format!("{}{}", base_url, path);

    let api_key = creds.get("apiKey").map(|s| s.as_str()).unwrap_or("");
    let auth_mode = creds.get("authMode").map(|s| s.as_str()).unwrap_or("bearer");
    let headers = build_headers(auth_mode, api_key);
    let client = http_client();

    match get_json(&client, &url, headers).await {
        Ok((status, data, raw)) => {
            if status == 401 || status == 403 {
                return raw_error(&format!("认证失败 (HTTP {})", status));
            }
            if status != 200 || data.is_none() {
                let snippet = &raw[..raw.len().min(200)];
                return raw_error(&format!("API错误 (HTTP {}): {}", status, snippet));
            }
            let data = data.unwrap();
            let parse_mode = creds.get("parseMode").map(|s| s.as_str()).unwrap_or("balance");

            if parse_mode == "balance" {
                let path_str = creds.get("availablePath").map(|s| s.as_str()).unwrap_or("data.available_balance");
                let available = get_path(&data, path_str).and_then(|v| parse_number(v)).unwrap_or(0.0);
                let balance = BalanceInfo {
                    available, voucher: 0.0, cash: 0.0, currency: "CNY".into(),
                };
                (vec![], Some(balance), None, None, Some(status))
            } else {
                let path_str = creds.get("usedPctPath").map(|s| s.as_str()).unwrap_or("data.used_percentage");
                let used_pct = get_path(&data, path_str).and_then(|v| parse_number(v)).unwrap_or(0.0);
                let tier = QuotaTier {
                    name: "five_hour".into(), label: "当前用量".into(),
                    used_percentage: used_pct.min(100.0).max(0.0),
                    used: None, limit: None, remaining: None,
                    resets_at: None, unit: Some("%".into()),
                };
                (vec![tier], None, None, None, Some(status))
            }
        }
        Err(e) => raw_error(&format!("网络错误: {}", e)),
    }
}

// ── 分发器 ────────────────────────────────────────────────

async fn query_raw(provider_id: &str, creds: &HashMap<String, String>) -> (Vec<QuotaTier>, Option<BalanceInfo>, Option<String>, Option<String>, Option<u16>) {
    match provider_id {
        "kimi_coding" => query_kimi_coding(creds).await,
        "kimi_balance" => query_kimi_balance(creds).await,
        "zhipu" => query_zhipu(creds).await,
        "minimax" => query_minimax(creds).await,
        "deepseek" => query_deepseek(creds).await,
        "custom" => query_custom(creds).await,
        _ => raw_error(&format!("未知的 Provider: {}", provider_id)),
    }
}

pub async fn query_one(config: &crate::config::ProviderConfig) -> UsageResult {
    let (tiers, balance, level, error, http_status) = query_raw(&config.provider_id, &config.creds).await;
    UsageResult {
        config_id: config.id.clone(),
        provider_id: config.provider_id.clone(),
        success: error.is_none(),
        tiers,
        balance,
        level,
        error,
        queried_at: now_ms(),
        http_status,
    }
}

pub async fn query_all(configs: &[crate::config::ProviderConfig]) -> Vec<UsageResult> {
    let futures: Vec<_> = configs.iter().map(|c| async move {
        let cid = c.id.clone();
        let pid = c.provider_id.clone();
        match tokio::time::timeout(Duration::from_secs(20), query_one(c)).await {
            Ok(result) => result,
            Err(_) => error_result(&cid, &pid, "查询超时"),
        }
    }).collect();
    futures::future::join_all(futures).await
}

// ── Provider 元信息 ───────────────────────────────────────

pub fn list_providers() -> Vec<ProviderMeta> {
    vec![
        ProviderMeta {
            id: "kimi_coding".into(),
            name: "Kimi 编程套餐".into(),
            description: "月之暗面 Kimi For Coding (5小时/周限额)".into(),
            auth_type: "bearer".into(),
            fields: vec![
                FieldDef { key: "apiKey".into(), label: "API Key".into(), field_type: "password".into(), required: true, placeholder: Some("sk-...".into()), default: None, options: None, description: None },
                FieldDef { key: "baseUrl".into(), label: "Base URL".into(), field_type: "text".into(), required: false, placeholder: None, default: Some("https://api.kimi.com".into()), options: None, description: None },
            ],
        },
        ProviderMeta {
            id: "zhipu".into(),
            name: "智谱 GLM".into(),
            description: "智谱 GLM Coding Plan (5小时/周限额)".into(),
            auth_type: "raw".into(),
            fields: vec![
                FieldDef { key: "apiKey".into(), label: "API Key".into(), field_type: "password".into(), required: true, placeholder: Some("xxx.xxx".into()), default: None, options: None, description: None },
                FieldDef { key: "baseUrl".into(), label: "Base URL".into(), field_type: "text".into(), required: false, placeholder: None, default: Some("https://open.bigmodel.cn".into()), options: None, description: None },
            ],
        },
        ProviderMeta {
            id: "minimax".into(),
            name: "MiniMax".into(),
            description: "MiniMax Coding Plan (5小时/周限额)".into(),
            auth_type: "bearer".into(),
            fields: vec![
                FieldDef { key: "apiKey".into(), label: "API Key".into(), field_type: "password".into(), required: true, placeholder: Some("...".into()), default: None, options: None, description: None },
                FieldDef { key: "baseUrl".into(), label: "Base URL".into(), field_type: "text".into(), required: false, placeholder: None, default: Some("https://api.minimaxi.com".into()), options: None, description: None },
            ],
        },
        ProviderMeta {
            id: "kimi_balance".into(),
            name: "Kimi 余额".into(),
            description: "月之暗面 Moonshot 按量付费余额".into(),
            auth_type: "bearer".into(),
            fields: vec![
                FieldDef { key: "apiKey".into(), label: "API Key".into(), field_type: "password".into(), required: true, placeholder: Some("sk-...".into()), default: None, options: None, description: None },
                FieldDef { key: "baseUrl".into(), label: "Base URL".into(), field_type: "text".into(), required: false, placeholder: None, default: Some("https://api.moonshot.cn".into()), options: None, description: None },
            ],
        },
        ProviderMeta {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            description: "DeepSeek 账户余额查询".into(),
            auth_type: "bearer".into(),
            fields: vec![
                FieldDef { key: "apiKey".into(), label: "API Key".into(), field_type: "password".into(), required: true, placeholder: Some("sk-...".into()), default: None, options: None, description: None },
                FieldDef { key: "baseUrl".into(), label: "Base URL".into(), field_type: "text".into(), required: false, placeholder: None, default: Some("https://api.deepseek.com".into()), options: None, description: None },
            ],
        },
        ProviderMeta {
            id: "custom".into(),
            name: "自定义".into(),
            description: "自定义用量/余额查询 (灵活配置)".into(),
            auth_type: "bearer".into(),
            fields: vec![
                FieldDef { key: "apiKey".into(), label: "API Key".into(), field_type: "password".into(), required: true, placeholder: Some("sk-...".into()), default: None, options: None, description: None },
                FieldDef { key: "baseUrl".into(), label: "Base URL".into(), field_type: "text".into(), required: true, placeholder: Some("https://api.example.com".into()), default: None, options: None, description: None },
                FieldDef { key: "path".into(), label: "查询路径".into(), field_type: "text".into(), required: true, placeholder: Some("/v1/balance".into()), default: None, options: None, description: None },
                FieldDef { key: "authMode".into(), label: "认证方式".into(), field_type: "select".into(), required: false, placeholder: None, default: Some("bearer".into()), options: Some(vec![
                    FieldOption { value: "bearer".into(), label: "Bearer Token".into() },
                    FieldOption { value: "raw".into(), label: "直接传 Key".into() },
                    FieldOption { value: "none".into(), label: "无认证".into() },
                ]), description: None },
                FieldDef { key: "parseMode".into(), label: "解析模式".into(), field_type: "select".into(), required: false, placeholder: None, default: Some("balance".into()), options: Some(vec![
                    FieldOption { value: "balance".into(), label: "余额模式".into() },
                    FieldOption { value: "percentage".into(), label: "百分比模式".into() },
                ]), description: None },
                FieldDef { key: "availablePath".into(), label: "余额字段路径".into(), field_type: "text".into(), required: false, placeholder: Some("data.available_balance".into()), default: None, options: None, description: None },
                FieldDef { key: "usedPctPath".into(), label: "已用百分比路径".into(), field_type: "text".into(), required: false, placeholder: Some("data.used_percentage".into()), default: None, options: None, description: None },
            ],
        },
    ]
}

pub fn get_groups() -> Vec<ProviderGroup> {
    let all = list_providers();
    let find = |id: &str| all.iter().find(|p| p.id == id).map(|p| ProviderInfo {
        id: p.id.clone(), name: p.name.clone(), description: p.description.clone(),
    });

    vec![
        ProviderGroup {
            id: "coding_plan".into(),
            label: "订阅套餐 (5h/7d)".into(),
            providers: ["kimi_coding", "zhipu", "minimax"].iter().filter_map(|id| find(id)).collect(),
        },
        ProviderGroup {
            id: "balance".into(),
            label: "余额查询".into(),
            providers: ["kimi_balance", "deepseek"].iter().filter_map(|id| find(id)).collect(),
        },
        ProviderGroup {
            id: "custom".into(),
            label: "自定义".into(),
            providers: vec![ProviderInfo {
                id: "custom".into(), name: "自定义".into(), description: "自定义用量/余额查询".into(),
            }],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zhipu_credit_limit_response() {
        let data: Value = serde_json::from_str(r#"{
            "code": 200, "msg": "Operation successful", "success": true,
            "data": {
                "level": "max",
                "limits": [
                    {"type":"CREDIT_LIMIT","unit":3,"number":5,"usage":28000,"currentValue":0,"remaining":28000,"percentage":0},
                    {"type":"CREDIT_LIMIT","unit":6,"number":1,"usage":140000,"currentValue":140016,"remaining":0,"percentage":100,"nextResetTime":1788326692998}
                ]
            }
        }"#).unwrap();

        let tiers = parse_zhipu_tiers(&data["data"]);
        assert_eq!(tiers.len(), 2);

        assert_eq!(tiers[0].name, "five_hour");
        assert_eq!(tiers[0].used_percentage, 0.0);
        assert_eq!(tiers[0].used, Some(0.0));
        assert_eq!(tiers[0].limit, Some(28000.0));
        assert_eq!(tiers[0].resets_at, None);

        assert_eq!(tiers[1].name, "weekly");
        assert_eq!(tiers[1].used_percentage, 100.0);
        assert_eq!(tiers[1].used, Some(140016.0));
        assert_eq!(tiers[1].limit, Some(140000.0));
        assert_eq!(tiers[1].unit.as_deref(), Some("积分"));
        assert!(tiers[1].resets_at.is_some());
    }

    #[test]
    fn zhipu_legacy_tokens_limit_response() {
        let data: Value = serde_json::from_str(r#"{
            "success": true,
            "data": {
                "level": "pro",
                "limits": [
                    {"type":"TOKENS_LIMIT","unit":3,"number":5,"usage":12000000,"currentValue":3000000,"remaining":9000000,"percentage":25},
                    {"type":"TOKENS_LIMIT","unit":6,"number":7,"usage":60000000,"currentValue":60000000,"remaining":0,"percentage":100,"nextResetTime":1788300000000}
                ]
            }
        }"#).unwrap();

        let tiers = parse_zhipu_tiers(&data["data"]);
        assert_eq!(tiers.len(), 2);
        assert_eq!(tiers[0].name, "five_hour");
        assert_eq!(tiers[0].used_percentage, 25.0);
        assert_eq!(tiers[1].name, "weekly");
        assert_eq!(tiers[1].used_percentage, 100.0);
    }

    #[test]
    fn zhipu_unclassified_falls_back_by_reset_time() {
        let data: Value = serde_json::from_str(r#"{
            "success": true,
            "data": {
                "limits": [
                    {"type":"CREDIT_LIMIT","unit":6,"percentage":40,"nextResetTime":2000},
                    {"type":"CREDIT_LIMIT","unit":3,"percentage":10,"nextResetTime":1000}
                ]
            }
        }"#).unwrap();

        let tiers = parse_zhipu_tiers(&data["data"]);
        assert_eq!(tiers.len(), 2);
        assert_eq!(tiers[0].name, "five_hour");
        assert_eq!(tiers[0].used_percentage, 10.0);
        assert_eq!(tiers[1].name, "weekly");
        assert_eq!(tiers[1].used_percentage, 40.0);
    }
}
