use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::header::{ACCEPT, COOKIE, HeaderMap, HeaderValue, LOCATION, SET_COOKIE};
use reqwest::{RequestBuilder, Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::config::{Cookie, Session};
use crate::error::ApolloError;
use crate::model::{App, EnvClusterInfo, Item, MultiResponse, Namespace, Page};

const PAGE_SIZE: usize = 20;
const MAX_PAGES: usize = 10_000;

#[derive(Clone)]
pub struct ApolloClient {
    base_url: Url,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct SigninForm<'a> {
    username: &'a str,
    password: &'a str,
    #[serde(rename = "login-submit")]
    login_submit: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateItem<'a> {
    table_view_oper_type: &'static str,
    key: &'a str,
    value: &'a str,
    add_item_btn_disabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NamespaceText<'a> {
    config_text: &'a str,
    namespace_id: u64,
    format: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseRequest<'a> {
    release_title: &'a str,
    release_comment: &'a str,
    is_emergency_publish: bool,
}

#[derive(Serialize)]
struct SearchQuery<'a> {
    query: &'a str,
    page: usize,
    size: usize,
}

impl ApolloClient {
    pub fn new(url: &str) -> Result<Self> {
        let base_url = parse_base_url(url)?;
        let http = build_http(None)?;
        Ok(Self { base_url, http })
    }

    pub fn with_session(&self, session: &Session) -> Result<Self> {
        Ok(Self {
            base_url: self.base_url.clone(),
            http: build_http(Some(session))?,
        })
    }

    pub fn base_url_string(&self) -> String {
        self.base_url.as_str().trim_end_matches('/').to_owned()
    }

    pub async fn signin(&self, username: &str, password: &str) -> Result<Session> {
        let response = self
            .http
            .post(self.endpoint(&["signin"])?)
            .form(&SigninForm {
                username,
                password,
                login_submit: "Login",
            })
            .send()
            .await
            .context("login request failed")?;
        let status = response.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(ApolloError::LoginFailed.into());
        }
        if status.is_redirection()
            && response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|location| location.contains("signin") || location.contains("error"))
        {
            return Err(ApolloError::LoginFailed.into());
        }
        if !status.is_success() && !status.is_redirection() {
            return Err(ApolloError::Http {
                status,
                message: "login was rejected".to_owned(),
            }
            .into());
        }

        let mut cookies = Vec::new();
        for header in response.headers().get_all(SET_COOKIE) {
            let header = header
                .to_str()
                .context("Apollo returned a non-text session cookie")?;
            let pair = header.split(';').next().unwrap_or_default();
            let (name, value) = pair
                .split_once('=')
                .context("Apollo returned a malformed session cookie")?;
            if name.trim().is_empty() {
                bail!("Apollo returned a session cookie with an empty name");
            }
            cookies.push(Cookie {
                name: name.trim().to_owned(),
                value: value.trim().to_owned(),
            });
        }
        if cookies.is_empty() {
            return Err(ApolloError::InvalidResponse(
                "login succeeded without returning a session cookie".to_owned(),
            )
            .into());
        }
        Ok(Session { cookies })
    }

    pub async fn current_user(&self) -> Result<String> {
        let value: Value = self.get_json(&["user"], "load current user").await?;
        if let Some(username) = value.as_str() {
            return Ok(username.to_owned());
        }
        for field in ["username", "userId", "name"] {
            if let Some(username) = value.get(field).and_then(Value::as_str) {
                return Ok(username.to_owned());
            }
        }
        Err(ApolloError::InvalidResponse(
            "the /user response did not contain a username".to_owned(),
        )
        .into())
    }

    pub async fn list_apps(&self, query: Option<&str>) -> Result<Vec<App>> {
        let mut apps = Vec::new();
        let mut page_number = 0;

        loop {
            if page_number >= MAX_PAGES {
                return Err(ApolloError::InvalidResponse(format!(
                    "app pagination exceeded {MAX_PAGES} pages"
                ))
                .into());
            }
            let request = self
                .http
                .get(self.endpoint(&["apps", "search", "by-appid-or-name"])?)
                .query(&SearchQuery {
                    query: query.unwrap_or_default(),
                    page: page_number,
                    size: PAGE_SIZE,
                });
            let page: Page<App> = self.send_json(request, "search apps").await?;
            if page.page != page_number {
                return Err(ApolloError::InvalidResponse(format!(
                    "requested app page {page_number}, but received page {}",
                    page.page
                ))
                .into());
            }

            let item_count = page.content.len();
            let total = page.total;
            apps.extend(page.content);
            if apps.len() as u64 >= total {
                return Ok(apps);
            }
            if item_count == 0 {
                return Err(ApolloError::InvalidResponse(format!(
                    "app pagination ended at {} of {total} results",
                    apps.len()
                ))
                .into());
            }
            page_number += 1;
        }
    }

    pub async fn nav_tree(&self, app_id: &str) -> Result<Vec<EnvClusterInfo>> {
        let response: MultiResponse<EnvClusterInfo> = self
            .get_json(&["apps", app_id, "navtree"], "load app navigation tree")
            .await?;
        if !(200..300).contains(&response.code) {
            return Err(ApolloError::InvalidResponse(format!(
                "navigation tree returned outer status {}",
                response.code
            ))
            .into());
        }

        let mut environments = Vec::new();
        for entity in response.entities {
            if !(200..300).contains(&entity.code) {
                let message = entity
                    .message
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown error".to_owned());
                return Err(ApolloError::InvalidResponse(format!(
                    "navigation tree entry returned {}: {message}",
                    entity.code
                ))
                .into());
            }
            let body = entity.body.ok_or_else(|| {
                ApolloError::InvalidResponse(
                    "navigation tree entry did not contain a body".to_owned(),
                )
            })?;
            environments.push(body);
        }
        Ok(environments)
    }

    pub async fn namespaces(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
    ) -> Result<Vec<Namespace>> {
        self.get_json(
            &[
                "apps",
                app_id,
                "envs",
                env,
                "clusters",
                cluster,
                "namespaces",
            ],
            "list namespaces",
        )
        .await
    }

    pub async fn update_item(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &str,
        item: &Item,
    ) -> Result<()> {
        let request = self
            .http
            .put(self.endpoint(&[
                "apps",
                app_id,
                "envs",
                env,
                "clusters",
                cluster,
                "namespaces",
                namespace,
                "item",
            ])?)
            .json(item);
        self.send_checked(request, "update configuration item")
            .await?;
        Ok(())
    }

    pub async fn create_item(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> Result<()> {
        let request = self
            .http
            .post(self.endpoint(&[
                "apps",
                app_id,
                "envs",
                env,
                "clusters",
                cluster,
                "namespaces",
                namespace,
                "item",
            ])?)
            .json(&CreateItem {
                table_view_oper_type: "create",
                key,
                value,
                add_item_btn_disabled: true,
            });
        self.send_checked(request, "create configuration item")
            .await?;
        Ok(())
    }

    pub async fn update_namespace(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &Namespace,
        format: &str,
        content: &str,
    ) -> Result<()> {
        let request = self
            .http
            .put(self.endpoint(&[
                "apps",
                app_id,
                "envs",
                env,
                "clusters",
                cluster,
                "namespaces",
                &namespace.base_info.namespace_name,
                "items",
            ])?)
            .json(&NamespaceText {
                config_text: content,
                namespace_id: namespace.base_info.id,
                format,
            });
        self.send_checked(request, "replace namespace content")
            .await?;
        Ok(())
    }

    pub async fn delete_item(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &str,
        item_id: u64,
    ) -> Result<()> {
        let item_id = item_id.to_string();
        let request = self.http.delete(self.endpoint(&[
            "apps",
            app_id,
            "envs",
            env,
            "clusters",
            cluster,
            "namespaces",
            namespace,
            "items",
            &item_id,
        ])?);
        self.send_checked(request, "delete configuration item")
            .await?;
        Ok(())
    }

    pub async fn release_namespace(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &str,
        title: &str,
        comment: &str,
    ) -> Result<()> {
        let request = self
            .http
            .post(self.endpoint(&[
                "apps",
                app_id,
                "envs",
                env,
                "clusters",
                cluster,
                "namespaces",
                namespace,
                "releases",
            ])?)
            .json(&ReleaseRequest {
                release_title: title,
                release_comment: comment,
                is_emergency_publish: false,
            });
        self.send_checked(request, "publish namespace").await?;
        Ok(())
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        operation: &'static str,
    ) -> Result<T> {
        let request = self.http.get(self.endpoint(segments)?);
        self.send_json(request, operation).await
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
        operation: &'static str,
    ) -> Result<T> {
        self.send_checked(request, operation)
            .await?
            .json::<T>()
            .await
            .map_err(|error| ApolloError::InvalidResponse(format!("{operation}: {error}")).into())
    }

    async fn send_checked(
        &self,
        request: RequestBuilder,
        operation: &'static str,
    ) -> Result<Response> {
        let response = request
            .send()
            .await
            .with_context(|| format!("{operation} request failed"))?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED
            || status == StatusCode::FORBIDDEN
            || status.is_redirection()
        {
            return Err(ApolloError::NotAuthenticated.into());
        }
        if !status.is_success() {
            let message = response.text().await.unwrap_or_else(|_| status.to_string());
            let message = message.chars().take(512).collect::<String>();
            return Err(ApolloError::Http { status, message }.into());
        }
        Ok(response)
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.base_url.clone();
        url.set_query(None);
        url.set_fragment(None);
        let mut path = url.path_segments_mut().map_err(|_| {
            ApolloError::InvalidResponse("the configured URL cannot contain a path".to_owned())
        })?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
        drop(path);
        Ok(url)
    }
}

fn parse_base_url(input: &str) -> Result<Url> {
    let mut url = Url::parse(input.trim()).context("invalid Apollo URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        bail!("Apollo URL must be an absolute HTTP or HTTPS URL");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("Apollo URL must not contain embedded credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("Apollo URL must not contain a query or fragment");
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url)
}

fn build_http(session: Option<&Session>) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    if let Some(cookie) = session.and_then(Session::cookie_header) {
        headers.insert(COOKIE, HeaderValue::from_str(&cookie)?);
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("apollo/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("could not create HTTP client")
}
