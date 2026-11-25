use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::Path;
use std::process::Output;
use std::sync::Mutex;

use apollo::client::ApolloClient;
use apollo::config::{Config, ConfigStore, Cookie, Session};
use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Match, Mock, MockServer, Request, Respond, ResponseTemplate};

fn apollo(config_dir: &Path) -> Command {
    let mut command = cargo_bin_cmd!("apollo");
    command.env("APOLLO_CONFIG_DIR", config_dir);
    command
}

fn test_session() -> Session {
    Session {
        cookies: vec![Cookie {
            name: "JSESSIONID".to_owned(),
            value: "cli-session-token".to_owned(),
        }],
    }
}

fn configure(config_dir: &Path, url: &str) {
    ConfigStore::at(config_dir)
        .save(&Config {
            url: url.to_owned(),
            session: Some(test_session()),
        })
        .unwrap();
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap()
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "stderr: {}", stderr(output));
}

fn app_values(prefix: &str, start: usize, count: usize) -> Vec<Value> {
    (start..start + count)
        .map(|index| {
            json!({
                "appId": format!("{prefix}-{index}"),
                "name": format!("App {index}"),
                "ownerName": "owner",
                "orgId": "org",
                "orgName": "Organization"
            })
        })
        .collect()
}

async fn mount_app_page(
    server: &MockServer,
    query: &str,
    page: usize,
    total: usize,
    prefix: &str,
    start: usize,
    count: usize,
) {
    let page_string = page.to_string();
    Mock::given(method("GET"))
        .and(path("/apps/search/by-appid-or-name"))
        .and(query_param("query", query))
        .and(query_param("page", page_string.as_str()))
        .and(query_param("size", "20"))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total": total,
            "content": app_values(prefix, start, count),
            "page": page,
            "size": 20
        })))
        .expect(1)
        .mount(server)
        .await;
}

fn namespace_body(value: &str) -> Value {
    json!([
        {
            "baseInfo": {
                "id": 8588,
                "namespaceName": "application",
                "appId": "sample-app",
                "clusterName": "default"
            },
            "items": [
                {
                    "item": {
                        "id": 51036,
                        "namespaceId": 8588,
                        "key": "dsn",
                        "value": value,
                        "comment": "keep this comment",
                        "lineNum": 1,
                        "type": 0,
                        "dataChangeCreatedBy": "fixture-user",
                        "dataChangeLastModifiedBy": "fixture-user"
                    },
                    "isModified": false
                }
            ],
            "format": "properties",
            "isConfigHidden": false
        },
        {
            "baseInfo": {
                "id": 8589,
                "namespaceName": "jkws.json",
                "appId": "sample-app",
                "clusterName": "default"
            },
            "items": [
                {
                    "item": {
                        "id": 51037,
                        "namespaceId": 8589,
                        "key": "content",
                        "value": "{\"name\":\"模块\",\"enabled\":true}",
                        "comment": null,
                        "lineNum": 1,
                        "type": 0
                    }
                }
            ],
            "format": "json",
            "isConfigHidden": false
        }
    ])
}

fn namespace_body_with_missing_item(value: &str) -> Value {
    let mut body = namespace_body("old");
    body[0]["items"].as_array_mut().unwrap().push(json!({
        "item": {
            "id": 51040,
            "namespaceId": 8588,
            "key": "missing",
            "value": value,
            "comment": null,
            "lineNum": 2,
            "type": 0
        }
    }));
    body
}

fn namespace_body_with_json_content(value: &str) -> Value {
    let mut body = namespace_body("old");
    body[1]["items"][0]["item"]["value"] = json!(value);
    body
}

fn namespace_body_without_dsn() -> Value {
    let mut body = namespace_body("old");
    body[0]["items"][0]["isDeleted"] = json!(true);
    body
}

fn is_generated_release_title(title: &str) -> bool {
    title.len() == 26
        && title.ends_with("-cli-release")
        && title.as_bytes()[..14].iter().all(u8::is_ascii_digit)
}

#[derive(Debug)]
struct AutomaticReleaseMatcher;

impl Match for AutomaticReleaseMatcher {
    fn matches(&self, request: &Request) -> bool {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        body["releaseTitle"]
            .as_str()
            .is_some_and(is_generated_release_title)
            && body["releaseComment"] == ""
            && body["isEmergencyPublish"] == false
    }
}

#[derive(Debug)]
struct SigninFormMatcher;

impl Match for SigninFormMatcher {
    fn matches(&self, request: &Request) -> bool {
        let fields: HashMap<String, String> = url::form_urlencoded::parse(&request.body)
            .into_owned()
            .collect();
        fields.get("username").map(String::as_str) == Some("cli-test-user")
            && fields.get("password").map(String::as_str) == Some("cli-test-password")
            && fields.get("login-submit").map(String::as_str) == Some("Login")
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_prompts_for_all_missing_values_and_status_reads_user() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();

    Mock::given(method("POST"))
        .and(path("/signin"))
        .and(SigninFormMatcher)
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/")
                .insert_header(
                    "Set-Cookie",
                    "JSESSIONID=cli-session-token; Path=/; HttpOnly",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "userId": "cli-test-user"
        })))
        .expect(3)
        .mount(&server)
        .await;

    let input = format!("{}\ncli-test-user\ncli-test-password\n", server.uri());
    let login = apollo(config_dir.path())
        .arg("login")
        .write_stdin(input)
        .output()
        .unwrap();
    assert_success(&login);
    assert!(stdout(&login).contains("loggedIn: true"));
    assert!(stdout(&login).contains("username: cli-test-user"));

    let config_path = config_dir.path().join("session.json");
    let saved = fs::read_to_string(&config_path).unwrap();
    assert!(!saved.contains("cli-test-user"));
    assert!(!saved.contains("cli-test-password"));
    assert!(saved.contains("cli-session-token"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(config_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    let status = apollo(config_dir.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert_success(&status);
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["url"], server.uri());
    assert_eq!(status["loggedIn"], true);
    assert_eq!(status["username"], "cli-test-user");

    let human = apollo(config_dir.path()).arg("status").output().unwrap();
    assert_success(&human);
    assert_eq!(
        stdout(&human),
        format!(
            "---\nurl: {}\nloggedIn: true\nusername: cli-test-user\n---\n",
            server.uri()
        )
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relogin_can_reuse_the_saved_url() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());

    Mock::given(method("POST"))
        .and(path("/signin"))
        .and(SigninFormMatcher)
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/")
                .insert_header(
                    "Set-Cookie",
                    "JSESSIONID=fresh-session-token; Path=/; HttpOnly",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("cookie", "JSESSIONID=fresh-session-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "userId": "cli-test-user"
        })))
        .expect(2)
        .mount(&server)
        .await;

    let login = apollo(config_dir.path())
        .arg("login")
        .write_stdin("y\ncli-test-user\ncli-test-password\n")
        .output()
        .unwrap();
    assert_success(&login);
    assert!(stdout(&login).contains("loggedIn: true"));
    assert!(stderr(&login).contains(&format!("Use last URL {}?", server.uri())));

    let status = apollo(config_dir.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert_success(&status);
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["url"], server.uri());
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relogin_asking_for_a_new_url_replaces_the_saved_one() {
    let old_server = MockServer::start().await;
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &old_server.uri());

    Mock::given(method("POST"))
        .and(path("/signin"))
        .and(SigninFormMatcher)
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "/")
                .insert_header(
                    "Set-Cookie",
                    "JSESSIONID=fresh-session-token; Path=/; HttpOnly",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "userId": "cli-test-user"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let input = format!("n\n{}\ncli-test-user\ncli-test-password\n", server.uri());
    let login = apollo(config_dir.path())
        .arg("login")
        .write_stdin(input)
        .output()
        .unwrap();
    assert_success(&login);
    assert!(stdout(&login).contains("loggedIn: true"));

    let saved = ConfigStore::at(config_dir.path()).load().unwrap().unwrap();
    assert_eq!(saved.url, server.uri());
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejected_login_does_not_save_credentials() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    Mock::given(method("POST"))
        .and(path("/signin"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/signin?#/error"))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "login",
            "--url",
            &server.uri(),
            "--username",
            "cli-test-user",
        ])
        .write_stdin("cli-test-password\n")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("login failed"));
    assert!(!config_dir.path().join("session.json").exists());
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_with_an_expired_session_reports_logged_out() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/signin"))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert_success(&output);
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["loggedIn"], false);
    assert_eq!(status["username"], Value::Null);
    server.verify().await;
}

#[tokio::test]
async fn status_without_configuration_is_machine_readable() {
    let config_dir = TempDir::new().unwrap();
    let output = apollo(config_dir.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert_success(&output);
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        status,
        json!({"url": null, "loggedIn": false, "username": null})
    );

    let human = apollo(config_dir.path()).arg("status").output().unwrap();
    assert_success(&human);
    assert_eq!(
        stdout(&human),
        "---\nurl: null\nloggedIn: false\nusername: null\n---\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn app_list_paginates_empty_and_keyword_queries() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());

    for query in ["", "mixer"] {
        let prefix = if query.is_empty() { "all" } else { "match" };
        mount_app_page(&server, query, 0, 21, prefix, 0, 20).await;
        mount_app_page(&server, query, 1, 21, prefix, 20, 1).await;
    }

    let all = apollo(config_dir.path())
        .args(["search", "--json"])
        .output()
        .unwrap();
    assert_success(&all);
    let all_apps: Value = serde_json::from_slice(&all.stdout).unwrap();
    assert_eq!(all_apps.as_array().unwrap().len(), 21);
    assert_eq!(all_apps[0]["appId"], "all-0");
    assert_eq!(all_apps[20]["appId"], "all-20");
    assert_eq!(all_apps[0]["ownerName"], "owner");

    let matches = apollo(config_dir.path())
        .args(["search", "mixer"])
        .output()
        .unwrap();
    assert_success(&matches);
    assert!(stdout(&matches).starts_with("---\napps:\n - appId: match-0\n   name: App 0\n"));
    assert!(stdout(&matches).contains(" - appId: match-20\n   name: App 20\n"));
    assert!(stdout(&matches).ends_with("---\n"));
    assert_eq!(stdout(&matches).lines().count(), 45);
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn app_tree_supports_human_and_json_output() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    Mock::given(method("GET"))
        .and(path("/apps/sample-app/navtree"))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 200,
            "entities": [
                {
                    "code": 200,
                    "message": "OK",
                    "body": {
                        "env": "DEV",
                        "clusters": [{"name": "default"}, {"name": "reports"}]
                    }
                },
                {
                    "code": 200,
                    "message": "OK",
                    "body": {"env": "PRO", "clusters": [{"name": "default"}]}
                }
            ]
        })))
        .expect(2)
        .mount(&server)
        .await;

    let human = apollo(config_dir.path())
        .args(["app", "sample-app", "--list"])
        .output()
        .unwrap();
    assert_success(&human);
    assert_eq!(
        stdout(&human),
        "---\napp: sample-app\nenvs:\n - name: DEV\n   clusters: [default, reports]\n - name: PRO\n   clusters: [default]\n---\n"
    );

    let json_output = apollo(config_dir.path())
        .args(["app", "sample-app", "--json"])
        .output()
        .unwrap();
    assert_success(&json_output);
    let tree: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(
        tree,
        json!({
            "app": "sample-app",
            "envs": [
                {"env": "DEV", "clusters": ["default", "reports"]},
                {"env": "PRO", "clusters": ["default"]}
            ]
        })
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn app_lists_namespaces_and_reads_one_by_its_full_name() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(5)
        .mount(&server)
        .await;

    let all = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "--json"])
        .output()
        .unwrap();
    assert_success(&all);
    let summaries: Value = serde_json::from_slice(&all.stdout).unwrap();
    assert_eq!(
        summaries,
        json!({
            "app": "sample-app",
            "env": "DEV",
            "cluster": "default",
            "namespaces": [
                {"namespace": "application", "format": "properties"},
                {"namespace": "jkws.json", "format": "json"}
            ]
        })
    );

    let human_namespaces = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "--list"])
        .output()
        .unwrap();
    assert_success(&human_namespaces);
    assert_eq!(
        stdout(&human_namespaces),
        "---\napp: sample-app\nenv: DEV\ncluster: default\nnamespaces:\n  - name: application\n    format: properties\n  - name: jkws.json\n    format: json\n---\n"
    );

    let raw = apollo(config_dir.path())
        .args(["sample-app", "DEV", "default", "jkws.json"])
        .output()
        .unwrap();
    assert_success(&raw);
    assert_eq!(
        stdout(&raw),
        "{\n  \"enabled\": true,\n  \"name\": \"模块\"\n}\n"
    );

    let filtered = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "jkws.json", "--meta"])
        .output()
        .unwrap();
    assert_success(&filtered);
    assert_eq!(
        stdout(&filtered),
        "---\napp: sample-app\nenv: DEV\ncluster: default\nnamespace: jkws.json\n---\n\n```json\n{\n  \"enabled\": true,\n  \"name\": \"模块\"\n}\n```\n"
    );
    assert!(!stdout(&filtered).contains("\\u"));

    let explicit_json = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "jkws.json", "--json"])
        .output()
        .unwrap();
    assert_success(&explicit_json);
    let output: Value = serde_json::from_slice(&explicit_json.stdout).unwrap();
    assert_eq!(
        output,
        json!({
            "app": "sample-app",
            "env": "DEV",
            "cluster": "default",
            "namespace": "jkws.json",
            "format": "json",
            "content": {"enabled": true, "name": "模块"}
        })
    );
    assert!(!stdout(&explicit_json).contains("\\u"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn namespace_json_output_falls_back_to_raw_content_for_non_object_json() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let mut without_content_item = namespace_body_with_json_content("{not json");
    without_content_item[1]["items"][0]["item"]["key"] = json!("config");
    without_content_item[1]["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "item": {
                "id": 51038, "namespaceId": 8589, "key": "other", "value": "x",
                "comment": null, "lineNum": 2, "type": 0
            }
        }));
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200)
                    .set_body_json(namespace_body_with_json_content("[1, 2]")),
                ResponseTemplate::new(200)
                    .set_body_json(namespace_body_with_json_content("{not json")),
                ResponseTemplate::new(200)
                    .set_body_json(namespace_body_with_json_content("{not json")),
                ResponseTemplate::new(200).set_body_json(without_content_item),
            ])),
        })
        .expect(4)
        .mount(&server)
        .await;

    // A JSON array is valid JSON but not an object, so the stored string is returned as-is.
    let array = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "jkws.json", "--json"])
        .output()
        .unwrap();
    assert_success(&array);
    let output: Value = serde_json::from_slice(&array.stdout).unwrap();
    assert_eq!(
        output,
        json!({
            "app": "sample-app",
            "env": "DEV",
            "cluster": "default",
            "namespace": "jkws.json",
            "format": "json",
            "rawContent": "[1, 2]"
        })
    );

    // Invalid JSON is not an error for `--json`; the stored string is returned as-is.
    let invalid = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "jkws.json", "--json"])
        .output()
        .unwrap();
    assert_success(&invalid);
    let output: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(output["rawContent"], "{not json");
    assert!(output.get("content").is_none());

    // The default output keeps rejecting invalid JSON.
    let raw = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "jkws.json"])
        .output()
        .unwrap();
    assert!(!raw.status.success());
    assert!(stderr(&raw).contains("contains invalid JSON"));

    // A JSON namespace without an identifiable content item is a structural error.
    let misplaced = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "jkws.json", "--json"])
        .output()
        .unwrap();
    assert!(!misplaced.status.success());
    assert!(stderr(&misplaced).contains("must contain a content item"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn properties_namespace_uses_sorted_minijinja_lines() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "baseInfo": {
                "id": 8588,
                "namespaceName": "application",
                "appId": "sample-app",
                "clusterName": "default"
            },
            "items": [
                {"item": {
                    "id": 2, "namespaceId": 8588, "key": "second", "value": "two",
                    "comment": "note", "lineNum": 10, "type": 0
                }},
                {"item": {
                    "id": 1, "namespaceId": 8588, "key": "first", "value": "值",
                    "comment": null, "lineNum": 2, "type": 0
                }},
                {"item": {
                    "id": 3, "namespaceId": 8588, "key": "", "value": "ignored",
                    "comment": "# standalone", "lineNum": 5, "type": 0
                }},
                {"item": {
                    "id": 4, "namespaceId": 8588, "key": "after", "value": "last",
                    "comment": "", "lineNum": 10, "type": 0
                }}
            ],
            "format": "properties",
            "isConfigHidden": false
        }])))
        .expect(3)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--meta",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        stdout(&output),
        "---\napp: sample-app\nenv: DEV\ncluster: default\nnamespace: application\n---\n\n```properties\nfirst = 值\n# standalone\nsecond = two # note \nafter = last\n```\n"
    );

    let raw = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "application"])
        .output()
        .unwrap();
    assert_success(&raw);
    assert_eq!(
        stdout(&raw),
        "first = 值\n# standalone\nsecond = two # note \nafter = last\n"
    );

    let json_output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--json",
        ])
        .output()
        .unwrap();
    assert_success(&json_output);
    let output: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(
        output,
        json!({
            "app": "sample-app",
            "env": "DEV",
            "cluster": "default",
            "namespace": "application",
            "format": "properties",
            "rawContent": "first = 值\n# standalone\nsecond = two # note \nafter = last\n"
        })
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn other_namespace_formats_keep_the_table_output() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let mut body = namespace_body("old");
    body[0]["format"] = json!("yaml");
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(3)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--meta",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        stdout(&output),
        "---\napp: sample-app\nenv: DEV\ncluster: default\nnamespace: application\n---\n\n```text\nNAMESPACE\tKEY\tVALUE\napplication\tdsn\told\n```\n"
    );

    let raw = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "application"])
        .output()
        .unwrap();
    assert_success(&raw);
    assert_eq!(
        stdout(&raw),
        "NAMESPACE\tKEY\tVALUE\napplication\tdsn\told\n"
    );

    let json_output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--json",
        ])
        .output()
        .unwrap();
    assert_success(&json_output);
    let output: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(
        output,
        json!({
            "app": "sample-app",
            "env": "DEV",
            "cluster": "default",
            "namespace": "application",
            "format": "yaml",
            "rawContent": "NAMESPACE\tKEY\tVALUE\napplication\tdsn\told\n"
        })
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn namespace_json_output_reports_an_empty_format_when_the_server_omits_it() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let mut body = namespace_body("old");
    body[0].as_object_mut().unwrap().remove("format");
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(2)
        .mount(&server)
        .await;

    let json_output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--json",
        ])
        .output()
        .unwrap();
    assert_success(&json_output);
    let output: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(
        output,
        json!({
            "app": "sample-app",
            "env": "DEV",
            "cluster": "default",
            "namespace": "application",
            "format": "",
            "rawContent": "NAMESPACE\tKEY\tVALUE\napplication\tdsn\told\n"
        })
    );

    let meta = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--meta",
        ])
        .output()
        .unwrap();
    assert_success(&meta);
    assert!(stdout(&meta).contains("\n```text\n"));
    server.verify().await;
}

struct SequenceResponder {
    responses: Mutex<VecDeque<ResponseTemplate>>,
}

impl Respond for SequenceResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra request")
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn item_set_updates_existing_item_and_verifies_read_back() {
    assert_item_set_succeeds(None, true, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn item_set_accepts_interactive_confirmation() {
    assert_item_set_succeeds(Some("yes\n"), false, true).await;
}

async fn assert_item_set_succeeds(confirmation: Option<&str>, yes: bool, show_values: bool) {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
                ResponseTemplate::new(200).set_body_json(namespace_body("new")),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{namespace_path}/application/item")))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .and(body_json(json!({
            "id": 51036,
            "namespaceId": 8588,
            "key": "dsn",
            "value": "new",
            "comment": "keep this comment",
            "lineNum": 1,
            "type": 0,
            "dataChangeCreatedBy": "fixture-user",
            "dataChangeLastModifiedBy": "fixture-user"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let mut command = apollo(config_dir.path());
    command.args([
        "app",
        "sample-app",
        "DEV",
        "default",
        "application",
        "--set",
        "dsn",
        "new",
    ]);
    if yes {
        command.arg("--yes");
    }
    if show_values {
        command.arg("--show-values");
    }
    if let Some(input) = confirmation {
        command.write_stdin(input);
    }
    let output = command.output().unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("result: updated"));
    if show_values {
        assert!(stderr(&output).contains("oldValue: \"old\""));
        assert!(stderr(&output).contains("newValue: \"new\""));
    } else {
        assert!(stderr(&output).contains("values: redacted"));
        assert!(!stderr(&output).contains("oldValue:"));
        assert!(!stderr(&output).contains("newValue:"));
    }
    if confirmation.is_some() {
        assert!(stderr(&output).contains("Apply this change?"));
    } else {
        assert!(!stderr(&output).contains("Apply this change?"));
    }
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn item_set_does_not_retry_when_read_back_mismatches() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
                ResponseTemplate::new(200).set_body_json(namespace_body("unexpected")),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{namespace_path}/application/item")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--set",
            "dsn",
            "new",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("not retrying"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn item_set_declined_confirmation_never_writes() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{namespace_path}/application/item")))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--set",
            "dsn",
            "new",
        ])
        .write_stdin("no\n")
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        stdout(&output),
        "---\napp: sample-app\nenv: DEV\ncluster: default\nnamespace: application\noperation: update\nkey: dsn\nvalues: redacted\nresult: aborted\n---\n"
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn item_set_creates_a_missing_key_and_verifies_read_back() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
                ResponseTemplate::new(200).set_body_json(namespace_body_with_missing_item("new")),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{namespace_path}/application/item")))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .and(body_json(json!({
            "tableViewOperType": "create",
            "key": "missing",
            "value": "new",
            "addItemBtnDisabled": true
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--set",
            "missing",
            "new",
            "--yes",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("result: created"));
    assert!(stderr(&output).contains("operation: create"));
    assert!(stderr(&output).contains("values: redacted"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn item_set_reads_sensitive_value_from_file_and_redacts_output() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    let secret = "super-secret\nsecond-line";
    let secret_path = config_dir.path().join("secret-value");
    fs::write(&secret_path, secret).unwrap();

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
                ResponseTemplate::new(200).set_body_json(namespace_body_with_missing_item(secret)),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{namespace_path}/application/item")))
        .and(body_json(json!({
            "tableViewOperType": "create",
            "key": "missing",
            "value": secret,
            "addItemBtnDisabled": true
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--set-file",
            "missing",
        ])
        .arg(&secret_path)
        .arg("--yes")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("result: created"));
    assert!(stderr(&output).contains("values: redacted"));
    assert!(!stdout(&output).contains(secret));
    assert!(!stderr(&output).contains(secret));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn namespace_update_replaces_properties_text() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    let content = "dsn = new # keep this comment \n";
    let content_path = config_dir.path().join("namespace.properties");
    fs::write(&content_path, content).unwrap();

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
                ResponseTemplate::new(200).set_body_json(namespace_body("new")),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{namespace_path}/application/items")))
        .and(body_json(json!({
            "configText": content,
            "namespaceId": 8588,
            "format": "properties"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--update-file",
        ])
        .arg(&content_path)
        .arg("--yes")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("result: updated"));
    assert!(stderr(&output).contains("operation: update"));
    assert!(stderr(&output).contains("format: properties"));
    assert!(stderr(&output).contains("values: redacted"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn namespace_update_rejects_properties_read_back_mismatch() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{namespace_path}/application/items")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--update",
            "dsn = new # keep this comment",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("read-back content differed; not retrying"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn namespace_update_formats_json_before_replacing() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    let formatted = "{\n  \"X\": \"Y\",\n  \"name\": \"模块\"\n}";

    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200)
                    .set_body_json(namespace_body_with_json_content("{\"old\":true}")),
                ResponseTemplate::new(200)
                    .set_body_json(namespace_body_with_json_content(formatted)),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{namespace_path}/jkws.json/items")))
        .and(body_json(json!({
            "configText": formatted,
            "namespaceId": 8589,
            "format": "json"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "jkws.json",
            "--update",
            "{\"name\":\"模块\",\"X\":\"Y\"}",
            "--show-values",
            "--yes",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("result: updated"));
    assert!(stderr(&output).contains("format: json"));
    assert!(stderr(&output).contains(r#"oldValue: "{\n  \"old\": true\n}\n""#));
    assert!(stderr(&output).contains(r#"newValue: "{\n  \"X\": \"Y\""#));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn namespace_update_rejects_invalid_json_before_writing() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "jkws.json",
            "--update",
            "not-json",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("invalid JSON"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn namespace_update_rejects_other_formats() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    let mut body = namespace_body("old");
    body[0]["format"] = json!("yaml");
    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--update",
            "dsn = new",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("only supports properties and json"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn set_rejects_json_namespace() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "jkws.json",
            "--set",
            "content",
            "new",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--set is only supported for properties"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deleted_items_are_not_rendered() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    let mut body = namespace_body("old");
    body[0]["items"][0]["isDeleted"] = json!(true);
    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args(["app", "sample-app", "DEV", "default", "application"])
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(stdout(&output), "");
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_removes_existing_properties_key() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(SequenceResponder {
            responses: Mutex::new(VecDeque::from([
                ResponseTemplate::new(200).set_body_json(namespace_body("old")),
                ResponseTemplate::new(200).set_body_json(namespace_body_without_dsn()),
            ])),
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{namespace_path}/application/items/51036")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--delete",
            "dsn",
            "--yes",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("result: deleted"));
    assert!(stdout(&output).contains("values: redacted"));
    assert!(stderr(&output).contains("operation: delete"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_missing_key_does_not_write() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    let namespace_path = "/apps/sample-app/envs/DEV/clusters/default/namespaces";
    Mock::given(method("GET"))
        .and(path(namespace_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--delete",
            "missing",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("nothing to delete"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_publishes_with_explicit_title_and_comment() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces/application/releases",
        ))
        .and(header("cookie", "JSESSIONID=cli-session-token"))
        .and(body_json(json!({
            "releaseTitle": "manual-release",
            "releaseComment": "release note",
            "isEmergencyPublish": false
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--release",
            "--title",
            "manual-release",
            "--comment",
            "release note",
            "--yes",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("result: released"));
    assert!(stdout(&output).contains("title: manual-release"));
    assert!(stdout(&output).contains("comment: release note"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_generates_title_and_defaults_comment_to_empty() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces/application/releases",
        ))
        .and(AutomaticReleaseMatcher)
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--release",
            "--yes",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    let title = stdout(&output)
        .lines()
        .find_map(|line| line.strip_prefix("title: "))
        .unwrap();
    assert!(is_generated_release_title(title));
    assert!(stdout(&output).contains("\ncomment: \n"));
    assert!(stdout(&output).contains("result: released"));
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_rejects_a_missing_namespace_before_posting() {
    let server = MockServer::start().await;
    let config_dir = TempDir::new().unwrap();
    configure(config_dir.path(), &server.uri());
    Mock::given(method("GET"))
        .and(path(
            "/apps/sample-app/envs/DEV/clusters/default/namespaces",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(namespace_body("old")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let output = apollo(config_dir.path())
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "missing",
            "--release",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(stderr(&output).contains("namespace `missing` was not found"));
    server.verify().await;
}

#[test]
fn usage_spec_is_generated_from_the_clap_command_tree() {
    let output = apollo(Path::new("unused"))
        .arg("__usage_spec__")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(stdout(&output).contains("apollo"));
    assert!(stdout(&output).contains("search"));
    assert!(stdout(&output).contains("login"));
}

#[test]
fn app_arguments_reject_incomplete_paths_and_invalid_writes() {
    let incomplete = apollo(Path::new("unused"))
        .args(["app", "sample-app", "DEV"])
        .output()
        .unwrap();
    assert!(!incomplete.status.success());
    assert!(stderr(&incomplete).to_ascii_lowercase().contains("cluster"));

    let malformed_set = apollo(Path::new("unused"))
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--set",
            "dsn",
        ])
        .output()
        .unwrap();
    assert!(!malformed_set.status.success());
    assert!(stderr(&malformed_set).contains("VALUE"));

    let empty_key = apollo(Path::new("unused"))
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--set",
            "",
            "new",
        ])
        .output()
        .unwrap();
    assert!(!empty_key.status.success());
    assert!(stderr(&empty_key).contains("KEY cannot be empty"));

    let empty_title = apollo(Path::new("unused"))
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--release",
            "--title",
            "",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!empty_title.status.success());
    assert!(stderr(&empty_title).contains("title cannot be empty"));

    let title_without_release = apollo(Path::new("unused"))
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--title",
            "manual-release",
        ])
        .output()
        .unwrap();
    assert!(!title_without_release.status.success());
    assert!(stderr(&title_without_release).contains("--release"));

    let yes_without_set = apollo(Path::new("unused"))
        .args([
            "app",
            "sample-app",
            "DEV",
            "default",
            "application",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!yes_without_set.status.success());
    assert!(stderr(&yes_without_set).contains("--set"));
}

#[test]
fn direct_client_rejects_urls_with_embedded_credentials() {
    assert!(ApolloClient::new("https://user:password@example.invalid").is_err());
}
