use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use tokio::{
    task::JoinHandle,
    time::{interval, Duration, MissedTickBehavior},
};
use tracing::{info, warn};

use crate::instagram::InstagramConnection;

const INSIGHTS_PULL_INTERVAL: Duration = Duration::from_secs(60 * 60);
const MEDIA_INSIGHTS_LIMIT: &str = "25";

#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct InstagramInsightSnapshot {
    pub id: i64,
    pub instagram_account_id: String,
    pub followers_count: i64,
    pub reach: i64,
    pub saves: i64,
    pub shares: i64,
    pub raw_payload: Value,
    pub captured_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewInstagramInsightSnapshot {
    pub instagram_account_id: String,
    pub followers_count: i64,
    pub reach: i64,
    pub saves: i64,
    pub shares: i64,
    pub raw_payload: Value,
    pub captured_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct InstagramInsightsPuller {
    client: reqwest::Client,
    graph_base_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PulledInstagramInsights {
    pub followers_count: i64,
    pub reach: i64,
    pub saves: i64,
    pub shares: i64,
    pub raw_payload: Value,
}

#[derive(Debug, Clone)]
pub struct InstagramInsightsJob {
    pool: PgPool,
    puller: InstagramInsightsPuller,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InstagramInsightsJobSummary {
    pub pulled: bool,
    pub skipped_disconnected: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct AccountFieldsResponse {
    followers_count: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AccountInsightsResponse {
    data: Vec<MetricSeries>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MediaListResponse {
    data: Vec<MediaInsightsNode>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MediaInsightsNode {
    id: String,
    insights: Option<AccountInsightsResponse>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MetricSeries {
    name: String,
    values: Vec<MetricValue>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MetricValue {
    value: Value,
    end_time: Option<DateTime<Utc>>,
}

impl InstagramInsightsPuller {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            graph_base_url: "https://graph.facebook.com".to_owned(),
        }
    }

    pub async fn pull(
        &self,
        connection: &InstagramConnection,
    ) -> Result<PulledInstagramInsights> {
        let normalized = ValidatedInstagramInsightsInput::from_connection(connection)?;
        let account_url = graph_endpoint(
            &self.graph_base_url,
            &normalized.graph_api_version,
            &normalized.instagram_account_id,
            None,
        )?;
        let account_response = self
            .client
            .get(account_url)
            .query(&[
                ("fields", "followers_count"),
                ("access_token", normalized.access_token.as_str()),
            ])
            .send()
            .await
            .context("failed to request Instagram account fields")?;
        let account: AccountFieldsResponse =
            graph_json_response(account_response, "account fields").await?;

        let reach_url = graph_endpoint(
            &self.graph_base_url,
            &normalized.graph_api_version,
            &normalized.instagram_account_id,
            Some("insights"),
        )?;
        let reach_response = self
            .client
            .get(reach_url)
            .query(&[
                ("metric", "reach"),
                ("period", "day"),
                ("access_token", normalized.access_token.as_str()),
            ])
            .send()
            .await
            .context("failed to request Instagram reach insights")?;
        let reach: AccountInsightsResponse =
            graph_json_response(reach_response, "reach insights").await?;

        let media_url = graph_endpoint(
            &self.graph_base_url,
            &normalized.graph_api_version,
            &normalized.instagram_account_id,
            Some("media"),
        )?;
        let media_response = self
            .client
            .get(media_url)
            .query(&[
                ("fields", "id,insights.metric(saved,shares)"),
                ("limit", MEDIA_INSIGHTS_LIMIT),
                ("access_token", normalized.access_token.as_str()),
            ])
            .send()
            .await
            .context("failed to request Instagram media insights")?;
        let media: MediaListResponse = graph_json_response(media_response, "media insights").await?;
        let saves = media_metric_sum(&media, &["saved", "saves"]);
        let shares = media_metric_sum(&media, &["shares"]);

        Ok(PulledInstagramInsights {
            followers_count: account.followers_count.unwrap_or(0),
            reach: latest_metric_value(&reach.data, "reach"),
            saves,
            shares,
            raw_payload: json!({
                "account": account,
                "reach": reach,
                "media": media,
            }),
        })
    }
}

impl Default for InstagramInsightsPuller {
    fn default() -> Self {
        Self::new()
    }
}

impl InstagramInsightsJob {
    pub fn new(pool: PgPool, puller: InstagramInsightsPuller) -> Self {
        Self { pool, puller }
    }

    pub async fn run_once(&self) -> Result<InstagramInsightsJobSummary> {
        let Some(connection) = active_instagram_connection(&self.pool).await? else {
            return Ok(InstagramInsightsJobSummary {
                skipped_disconnected: true,
                ..InstagramInsightsJobSummary::default()
            });
        };

        pull_and_store_instagram_insights(&self.pool, &self.puller, &connection).await?;

        Ok(InstagramInsightsJobSummary {
            pulled: true,
            skipped_disconnected: false,
        })
    }
}

pub async fn pull_and_store_instagram_insights(
    pool: &PgPool,
    puller: &InstagramInsightsPuller,
    connection: &InstagramConnection,
) -> Result<InstagramInsightSnapshot> {
    let insights = puller.pull(connection).await?;

    create_instagram_insight_snapshot(
        pool,
        &NewInstagramInsightSnapshot {
            instagram_account_id: connection.instagram_account_id.clone(),
            followers_count: insights.followers_count,
            reach: insights.reach,
            saves: insights.saves,
            shares: insights.shares,
            raw_payload: insights.raw_payload,
            captured_at: Some(Utc::now()),
        },
    )
    .await
}

pub async fn create_instagram_insight_snapshot(
    pool: &PgPool,
    snapshot: &NewInstagramInsightSnapshot,
) -> Result<InstagramInsightSnapshot> {
    let normalized = ValidatedInstagramInsightSnapshot::from_new(snapshot)?;

    sqlx::query_as::<_, InstagramInsightSnapshot>(
        r#"
        INSERT INTO instagram_insight_snapshots (
            instagram_account_id,
            followers_count,
            reach,
            saves,
            shares,
            raw_payload,
            captured_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, NOW()))
        RETURNING
            id,
            instagram_account_id,
            followers_count,
            reach,
            saves,
            shares,
            raw_payload,
            captured_at,
            created_at
        "#,
    )
    .bind(&normalized.instagram_account_id)
    .bind(normalized.followers_count)
    .bind(normalized.reach)
    .bind(normalized.saves)
    .bind(normalized.shares)
    .bind(&normalized.raw_payload)
    .bind(normalized.captured_at)
    .persistent(false)
    .fetch_one(pool)
    .await
    .context("failed to create Instagram insight snapshot")
}

pub async fn list_instagram_insight_snapshots(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<InstagramInsightSnapshot>> {
    let bounded_limit = limit.clamp(1, 100);

    sqlx::query_as::<_, InstagramInsightSnapshot>(
        r#"
        SELECT
            id,
            instagram_account_id,
            followers_count,
            reach,
            saves,
            shares,
            raw_payload,
            captured_at,
            created_at
        FROM instagram_insight_snapshots
        ORDER BY captured_at DESC, id DESC
        LIMIT $1
        "#,
    )
    .bind(bounded_limit)
    .persistent(false)
    .fetch_all(pool)
    .await
    .context("failed to list Instagram insight snapshots")
}

pub async fn active_instagram_connection(pool: &PgPool) -> Result<Option<InstagramConnection>> {
    Ok(crate::instagram::find_instagram_connection(pool)
        .await?
        .filter(|connection| connection.disconnected_at.is_none()))
}

pub fn spawn_instagram_insights_job(
    pool: PgPool,
    puller: InstagramInsightsPuller,
) -> Result<JoinHandle<()>> {
    let job = InstagramInsightsJob::new(pool, puller);

    Ok(tokio::spawn(async move {
        let mut ticker = interval(INSIGHTS_PULL_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            match job.run_once().await {
                Ok(summary) if summary.pulled => {
                    info!("Instagram insights pull completed");
                }
                Ok(summary) if summary.skipped_disconnected => {
                    warn!("Instagram insights pull skipped because account is disconnected");
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(error = ?error, "Instagram insights pull failed");
                }
            }
        }
    }))
}

#[derive(Debug, Clone, PartialEq)]
struct ValidatedInstagramInsightsInput {
    instagram_account_id: String,
    access_token: String,
    graph_api_version: String,
}

impl ValidatedInstagramInsightsInput {
    fn from_connection(connection: &InstagramConnection) -> Result<Self> {
        Ok(Self {
            instagram_account_id: required_trimmed(
                &connection.instagram_account_id,
                "Instagram account id is required",
            )?,
            access_token: required_trimmed(
                &connection.access_token,
                "Instagram access token is required",
            )?,
            graph_api_version: required_trimmed(
                &connection.graph_api_version,
                "Instagram Graph API version is required",
            )?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ValidatedInstagramInsightSnapshot {
    instagram_account_id: String,
    followers_count: i64,
    reach: i64,
    saves: i64,
    shares: i64,
    raw_payload: Value,
    captured_at: Option<DateTime<Utc>>,
}

impl ValidatedInstagramInsightSnapshot {
    fn from_new(snapshot: &NewInstagramInsightSnapshot) -> Result<Self> {
        let instagram_account_id = required_trimmed(
            &snapshot.instagram_account_id,
            "Instagram account id is required",
        )?;

        if snapshot.followers_count < 0
            || snapshot.reach < 0
            || snapshot.saves < 0
            || snapshot.shares < 0
        {
            anyhow::bail!("Instagram insight counts must be non-negative");
        }

        Ok(Self {
            instagram_account_id,
            followers_count: snapshot.followers_count,
            reach: snapshot.reach,
            saves: snapshot.saves,
            shares: snapshot.shares,
            raw_payload: snapshot.raw_payload.clone(),
            captured_at: snapshot.captured_at,
        })
    }
}

async fn graph_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
    operation: &str,
) -> Result<T> {
    let status = response.status();

    if status.is_success() {
        return response
            .json::<T>()
            .await
            .with_context(|| format!("failed to decode Instagram Graph API {operation} response"));
    }

    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "response body unavailable".to_owned());

    match status {
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            anyhow::bail!("Instagram Graph API {operation} request was rejected: {body}");
        }
        _ => {
            anyhow::bail!(
                "Instagram Graph API {operation} request failed with status {status}: {body}"
            );
        }
    }
}

fn graph_endpoint(
    base_url: &str,
    graph_api_version: &str,
    instagram_account_id: &str,
    edge: Option<&str>,
) -> Result<Url> {
    let mut url = Url::parse(base_url).context("Instagram Graph API base URL is invalid")?;
    let graph_api_version = required_trimmed(
        graph_api_version,
        "Instagram Graph API version is required",
    )?;
    let instagram_account_id =
        required_trimmed(instagram_account_id, "Instagram account id is required")?;
    let edge = match edge {
        Some(edge) => Some(required_trimmed(
            edge,
            "Instagram Graph API edge is required",
        )?),
        None => None,
    };

    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Instagram Graph API base URL cannot be a base"))?;
        segments.clear();
        segments.push(&graph_api_version);
        segments.push(&instagram_account_id);

        if let Some(edge) = edge {
            segments.push(&edge);
        }
    }

    Ok(url)
}

fn latest_metric_value(metrics: &[MetricSeries], metric_name: &str) -> i64 {
    metrics
        .iter()
        .find(|metric| metric.name == metric_name)
        .and_then(|metric| metric.values.last())
        .map(|value| numeric_value(&value.value))
        .unwrap_or(0)
}

fn media_metric_sum(media: &MediaListResponse, metric_names: &[&str]) -> i64 {
    media.data
        .iter()
        .filter_map(|node| node.insights.as_ref())
        .flat_map(|insights| insights.data.iter())
        .filter(|metric| metric_names.iter().any(|name| metric.name == *name))
        .map(|metric| {
            metric
                .values
                .last()
                .map(|value| numeric_value(&value.value))
                .unwrap_or(0)
        })
        .sum()
}

fn numeric_value(value: &Value) -> i64 {
    if let Some(value) = value.as_i64() {
        return value.max(0);
    }

    if let Some(value) = value.as_u64() {
        return i64::try_from(value).unwrap_or(i64::MAX);
    }

    value
        .as_object()
        .map(|object| object.values().map(numeric_value).sum())
        .unwrap_or(0)
}

fn required_trimmed(value: &str, message: &'static str) -> Result<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        anyhow::bail!(message);
    }

    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use super::{
        graph_endpoint, latest_metric_value, media_metric_sum, AccountInsightsResponse,
        MediaInsightsNode, MediaListResponse, MetricSeries, MetricValue,
        NewInstagramInsightSnapshot, ValidatedInstagramInsightSnapshot,
    };

    #[test]
    fn reads_latest_reach_metric_value() {
        let metrics = vec![MetricSeries {
            name: "reach".to_owned(),
            values: vec![
                MetricValue {
                    value: json!(10),
                    end_time: None,
                },
                MetricValue {
                    value: json!(42),
                    end_time: None,
                },
            ],
        }];

        assert_eq!(latest_metric_value(&metrics, "reach"), 42);
    }

    #[test]
    fn sums_media_saved_and_share_metrics() {
        let media = MediaListResponse {
            data: vec![
                MediaInsightsNode {
                    id: "media-1".to_owned(),
                    insights: Some(AccountInsightsResponse {
                        data: vec![
                            MetricSeries {
                                name: "saved".to_owned(),
                                values: vec![MetricValue {
                                    value: json!(3),
                                    end_time: None,
                                }],
                            },
                            MetricSeries {
                                name: "shares".to_owned(),
                                values: vec![MetricValue {
                                    value: json!(2),
                                    end_time: None,
                                }],
                            },
                        ],
                    }),
                },
                MediaInsightsNode {
                    id: "media-2".to_owned(),
                    insights: Some(AccountInsightsResponse {
                        data: vec![MetricSeries {
                            name: "saved".to_owned(),
                            values: vec![MetricValue {
                                value: json!(5),
                                end_time: None,
                            }],
                        }],
                    }),
                },
            ],
        };

        assert_eq!(media_metric_sum(&media, &["saved", "saves"]), 8);
        assert_eq!(media_metric_sum(&media, &["shares"]), 2);
    }

    #[test]
    fn rejects_negative_snapshot_counts() {
        let snapshot = NewInstagramInsightSnapshot {
            instagram_account_id: "17841400000000000".to_owned(),
            followers_count: 100,
            reach: -1,
            saves: 0,
            shares: 0,
            raw_payload: json!({}),
            captured_at: Some(Utc::now()),
        };

        let error = match ValidatedInstagramInsightSnapshot::from_new(&snapshot) {
            Ok(_) => panic!("negative insight counts should fail validation"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("non-negative"));
    }

    #[test]
    fn builds_graph_endpoint_without_query_credentials() {
        let endpoint = match graph_endpoint(
            "https://graph.facebook.com",
            "v20.0",
            "17841400000000000",
            Some("insights"),
        ) {
            Ok(endpoint) => endpoint,
            Err(error) => panic!("valid graph endpoint should build: {error}"),
        };

        assert_eq!(
            endpoint.as_str(),
            "https://graph.facebook.com/v20.0/17841400000000000/insights"
        );
        assert!(endpoint.query().is_none());
    }
}
