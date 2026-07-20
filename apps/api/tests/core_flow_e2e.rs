use std::{
    env,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use api::{
    config::ObjectStorageConfig,
    db,
    drafts::{self, DraftStatus, NewPostDraft, UpdatePostDraft},
    instagram::{self, InstagramAccountType, NewInstagramConnection},
    publishing::{self, InstagramPublishTarget, InstagramPublisher, PublishLogStatus},
    schedule::{self, NewScheduleAssignment},
    sources::{self, ContentSourceKind, NewContentSource, NewIngestedItem},
    storage::ObjectStorage,
};
use chrono::{Days, Utc};
use sqlx::{postgres::PgPoolOptions, PgPool};
use url::Url;

const TEST_ACCOUNT_ID: &str = "17841400000000000";

#[tokio::test]
async fn core_content_flow_publishes_with_mocked_instagram_graph() -> Result<()> {
    let Some(database) = TestDatabase::connect_or_skip().await? else {
        return Ok(());
    };
    let graph = MockGraphApi::spawn()?;
    let storage = ObjectStorage::from_config(&ObjectStorageConfig {
        endpoint: "https://cdn.example.test".to_owned(),
        region: "auto".to_owned(),
        bucket: "public-assets".to_owned(),
        access_key_id: "test-access-key".to_owned(),
        secret_access_key: "test-secret-key".to_owned(),
        prefix: "e2e-assets".to_owned(),
    })
    .await?;
    let run_id = database.schema_name();

    let source = sources::create_content_source(
        database.pool(),
        &NewContentSource {
            name: format!("E2E Vancouver source {run_id}"),
            kind: ContentSourceKind::Website,
            url: Some("https://events.example.test/vancouver".to_owned()),
            external_id: None,
            created_by_sub: None,
        },
    )
    .await?;
    let item = sources::upsert_ingested_item(
        database.pool(),
        &NewIngestedItem {
            source_id: source.id,
            title: "Free waterfront jazz night".to_owned(),
            summary: Some("Outdoor music, food trucks, and sunset views in Vancouver.".to_owned()),
            link: "https://events.example.test/vancouver/jazz-night".to_owned(),
            media_ref: Some("source/jazz-night.jpg".to_owned()),
            dedup_key: format!("e2e-jazz-night-{run_id}"),
            source_published_at: Some(Utc::now()),
        },
    )
    .await?;

    let draft = drafts::create_post_draft(
        database.pool(),
        &NewPostDraft {
            source_item_id: Some(item.id),
            caption_en: "Free waterfront jazz night is worth catching this week.".to_owned(),
            caption_zh: "本周值得留意的海边免费爵士夜。".to_owned(),
            status: Some(DraftStatus::Draft),
            rendered_post_asset_ref: Some("e2e-assets/rendered/jazz-night-post.svg".to_owned()),
            rendered_reel_asset_ref: Some("e2e-assets/rendered/jazz-night-reel.mp4".to_owned()),
            created_by_sub: None,
        },
    )
    .await?;
    let approved = drafts::update_post_draft(
        database.pool(),
        draft.id,
        &UpdatePostDraft {
            source_item_id: None,
            caption_en: None,
            caption_zh: None,
            status: Some(DraftStatus::Approved),
            rendered_post_asset_ref: None,
            rendered_reel_asset_ref: None,
            updated_by_sub: None,
        },
    )
    .await?
    .context("approved draft should be returned")?;

    assert_eq!(approved.status, DraftStatus::Approved);

    let slot_date = Utc::now()
        .date_naive()
        .checked_add_days(Days::new(7))
        .context("future schedule date should be valid")?;
    let slot = schedule::assign_approved_draft_to_slot(
        database.pool(),
        &NewScheduleAssignment {
            slot_date,
            slot_time: None,
            draft_id: approved.id,
            user_sub: None,
        },
    )
    .await?;

    assert_eq!(slot.draft_id, Some(approved.id));

    let scheduled = drafts::find_post_draft(database.pool(), approved.id)
        .await?
        .context("scheduled draft should exist")?;
    assert_eq!(scheduled.status, DraftStatus::Scheduled);

    instagram::connect_instagram_account(
        database.pool(),
        &NewInstagramConnection {
            instagram_account_id: TEST_ACCOUNT_ID.to_owned(),
            username: Some("vancouverpuls_e2e".to_owned()),
            account_type: InstagramAccountType::Business,
            graph_api_version: "v20.0".to_owned(),
            app_id: "e2e-app".to_owned(),
            access_token: "sandbox-token".to_owned(),
            token_source: "e2e".to_owned(),
            connected_by_sub: None,
        },
    )
    .await?;
    let connection = instagram::find_instagram_connection(database.pool())
        .await?
        .context("Instagram connection should exist")?;
    let publisher = InstagramPublisher::with_graph_base_url(graph.base_url());

    let published = publishing::publish_draft_to_instagram(
        database.pool(),
        &storage,
        &publisher,
        &scheduled,
        &connection,
        InstagramPublishTarget::Post,
        Some("e2e-test"),
    )
    .await?;

    assert_eq!(published.draft.status, DraftStatus::Published);
    assert_eq!(published.log.status, PublishLogStatus::Success);
    assert_eq!(
        published.log.graph_container_id.as_deref(),
        Some("mock-container-id")
    );
    assert_eq!(published.log.graph_media_id.as_deref(), Some("mock-media-id"));

    let logs = publishing::list_publish_logs_for_draft(database.pool(), published.draft.id).await?;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status, PublishLogStatus::Success);

    let requests = graph.requests()?;
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].contains(&format!("POST /v20.0/{TEST_ACCOUNT_ID}/media ")),
        "media request should target the sandbox Graph media endpoint: {}",
        requests[0]
    );
    assert!(
        requests[0].contains("image_url=https%3A%2F%2Fcdn.example.test%2Fpublic-assets%2Fe2e-assets%2Frendered%2Fjazz-night-post.svg"),
        "media request should include the rendered post URL: {}",
        requests[0]
    );
    assert!(
        requests[0].contains("access_token=sandbox-token"),
        "media request should include the sandbox access token"
    );
    assert!(
        requests[1].contains(&format!("POST /v20.0/{TEST_ACCOUNT_ID}/media_publish ")),
        "publish request should target the sandbox Graph publish endpoint: {}",
        requests[1]
    );
    assert!(
        requests[1].contains("creation_id=mock-container-id"),
        "publish request should use the mocked container id"
    );

    graph.join()?;
    database.drop_schema().await?;

    Ok(())
}

struct TestDatabase {
    admin_pool: PgPool,
    pool: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn connect_or_skip() -> Result<Option<Self>> {
        let database_url =
            env::var("DATABASE_URL").context("DATABASE_URL must be set for the E2E test")?;
        let admin_pool = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!(
                    "skipping core flow E2E test because DATABASE_URL is unreachable: {error}"
                );
                return Ok(None);
            }
        };
        let schema = unique_schema_name()?;

        sqlx::query(&format!(r#"CREATE SCHEMA "{}""#, schema))
            .persistent(false)
            .execute(&admin_pool)
            .await
            .with_context(|| format!("failed to create test schema `{schema}`"))?;

        let schema_url = database_url_for_test_schema(&database_url, &schema)?;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&schema_url)
            .await
            .with_context(|| format!("failed to connect to test schema `{schema}`"))?;

        db::migrate(&pool).await?;

        Ok(Some(Self {
            admin_pool,
            pool,
            schema,
        }))
    }

    fn pool(&self) -> &PgPool {
        &self.pool
    }

    fn schema_name(&self) -> &str {
        &self.schema
    }

    async fn drop_schema(self) -> Result<()> {
        self.pool.close().await;
        sqlx::query(&format!(r#"DROP SCHEMA "{}" CASCADE"#, self.schema))
            .persistent(false)
            .execute(&self.admin_pool)
            .await
            .with_context(|| format!("failed to drop test schema `{}`", self.schema))?;
        self.admin_pool.close().await;

        Ok(())
    }
}

struct MockGraphApi {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    handle: thread::JoinHandle<std::io::Result<()>>,
}

impl MockGraphApi {
    fn spawn() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").context("failed to bind mock Graph API")?;
        let address = listener
            .local_addr()
            .context("failed to read mock Graph API address")?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            for response_id in ["mock-container-id", "mock-media-id"] {
                let (mut stream, _) = listener.accept()?;
                let request = read_http_request(&mut stream)?;
                {
                    let mut requests = thread_requests
                        .lock()
                        .map_err(|_| std::io::Error::other("request lock poisoned"))?;
                    requests.push(request);
                }
                write_json_response(&mut stream, response_id)?;
            }

            Ok(())
        });

        Ok(Self {
            base_url: format!("http://{address}"),
            requests,
            handle,
        })
    }

    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    fn requests(&self) -> Result<Vec<String>> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .map_err(|_| anyhow::anyhow!("mock Graph API request lock poisoned"))
    }

    fn join(self) -> Result<()> {
        self.handle
            .join()
            .map_err(|_| anyhow::anyhow!("mock Graph API thread panicked"))?
            .context("mock Graph API failed")
    }
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..count]);

        if let Some(header_end) = find_header_end(&buffer) {
            let headers = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
            let content_length = content_length(&headers);
            let body_read = buffer.len().saturating_sub(header_end + 4);

            if body_read >= content_length {
                break;
            }
        }
    }

    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn write_json_response(stream: &mut TcpStream, id: &str) -> std::io::Result<()> {
    let body = format!(r#"{{"id":"{id}"}}"#);
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn unique_schema_name() -> Result<String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before unix epoch")?
        .as_millis();

    Ok(format!("e2e_core_flow_{}_{}", std::process::id(), millis))
}

fn database_url_for_test_schema(database_url: &str, schema: &str) -> Result<String> {
    let mut url = Url::parse(database_url).context("DATABASE_URL is not a valid URL")?;
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));

    Ok(url.to_string())
}
