use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::cli::{AppArgs, Cli, Command, KeyValue, LoginArgs, SearchArgs};
use crate::client::ApolloClient;
use crate::config::{Config, ConfigStore};
use crate::error::ApolloError;
use crate::model::{
    AppEnvironment, AppOutput, ContentOutput, Item, Namespace, NamespaceOutput, NamespacesOutput,
};
use crate::{output, prompt};

pub async fn execute(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Login(arguments) => login(arguments).await,
        Command::Status(arguments) => status(arguments.json).await,
        Command::Search(arguments) => search(arguments).await,
        Command::App(arguments) => app(*arguments).await,
        Command::Direct(arguments) => direct(arguments).await,
        Command::UsageSpec => {
            io::stdout().write_all(crate::cli::usage_spec().as_bytes())?;
            Ok(())
        }
    }
}

async fn direct(arguments: Vec<OsString>) -> Result<()> {
    let mut positional = Vec::new();
    let mut meta = false;
    let mut json = false;
    for argument in arguments {
        match argument.to_str() {
            Some("--meta") => meta = true,
            Some("--json") => json = true,
            Some(value) if value.starts_with('-') => bail!("unsupported option `{value}`"),
            Some(value) => positional.push(value.to_owned()),
            None => bail!("direct namespace arguments must be valid UTF-8"),
        }
    }
    if meta && json {
        bail!("--meta and --json cannot be used together");
    }
    if positional.len() != 4 {
        bail!("expected `apollo APP ENV CLUSTER NAMESPACE [--meta|--json]`");
    }
    let mut positional = positional.into_iter();
    app(AppArgs {
        app_id: positional.next().expect("four positional arguments"),
        env: positional.next(),
        cluster: positional.next(),
        namespace: positional.next(),
        list: false,
        meta,
        set: None,
        set_file: None,
        update: None,
        update_file: None,
        delete: None,
        release: false,
        title: None,
        comment: None,
        yes: false,
        show_values: false,
        json,
    })
    .await
}

async fn login(arguments: LoginArgs) -> Result<()> {
    let url = match arguments.url {
        Some(url) => url,
        None => prompt::required("Apollo URL")?,
    };
    let client = ApolloClient::new(&url)?;
    let username = match arguments.username {
        Some(username) if !username.trim().is_empty() => username,
        Some(_) => bail!("Username cannot be empty"),
        None => prompt::required("Username")?,
    };
    let password = prompt::password()?;
    let session = client.signin(&username, password.as_str()).await?;
    let authenticated = client.with_session(&session)?;
    let server_username = authenticated.current_user().await?;

    let config = Config {
        url: client.base_url_string(),
        session: Some(session),
    };
    ConfigStore::discover()?.save(&config)?;
    output::status(&StatusOutput {
        url: Some(&config.url),
        logged_in: true,
        username: Some(&server_username),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusOutput<'a> {
    url: Option<&'a str>,
    logged_in: bool,
    username: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetOutput<'a> {
    app: &'a str,
    env: &'a str,
    cluster: &'a str,
    namespace: &'a str,
    operation: &'a str,
    key: &'a str,
    show_values: bool,
    old_value: Option<String>,
    new_value: Option<&'a str>,
    result: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NamespaceUpdateOutput<'a> {
    app: &'a str,
    env: &'a str,
    cluster: &'a str,
    namespace: &'a str,
    operation: &'a str,
    format: &'a str,
    show_values: bool,
    old_value: Option<&'a str>,
    new_value: Option<&'a str>,
    result: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseOutput<'a> {
    app: &'a str,
    env: &'a str,
    cluster: &'a str,
    namespace: &'a str,
    title: &'a str,
    comment: &'a str,
    result: Option<String>,
}

async fn status(json: bool) -> Result<()> {
    let config = ConfigStore::discover()?.load()?;
    let mut username = None;
    let mut logged_in = false;

    if let Some(config) = &config
        && let Some(session) = &config.session
    {
        let client = ApolloClient::new(&config.url)?.with_session(session)?;
        match client.current_user().await {
            Ok(current_username) => {
                username = Some(current_username);
                logged_in = true;
            }
            Err(error)
                if error
                    .downcast_ref::<ApolloError>()
                    .is_some_and(|error| matches!(error, ApolloError::NotAuthenticated)) => {}
            Err(error) => return Err(error),
        }
    }

    let status = StatusOutput {
        url: config.as_ref().map(|config| config.url.as_str()),
        logged_in,
        username: username.as_deref(),
    };
    if json {
        output::json(&status)
    } else {
        output::status(&status)
    }
}

async fn search(arguments: SearchArgs) -> Result<()> {
    let apps = configured_client()?
        .list_apps(arguments.query.as_deref())
        .await?;
    if arguments.json {
        output::json(&apps)
    } else {
        output::apps(&apps)
    }
}

async fn app(arguments: AppArgs) -> Result<()> {
    let change = match arguments.set.as_deref() {
        Some([key, value]) if !key.is_empty() => Some(KeyValue {
            key: key.clone(),
            value: value.clone(),
        }),
        Some([_, _]) => bail!("KEY cannot be empty"),
        Some(_) => bail!("--set requires KEY and VALUE"),
        None => match arguments.set_file.as_deref() {
            Some([key, path]) if !key.is_empty() => Some(KeyValue {
                key: key.clone(),
                value: fs::read_to_string(path)
                    .with_context(|| format!("could not read configuration value from `{path}`"))?,
            }),
            Some([_, _]) => bail!("KEY cannot be empty"),
            Some(_) => bail!("--set-file requires KEY and FILE"),
            None => None,
        },
    };
    let update = match (&arguments.update, &arguments.update_file) {
        (Some(content), None) => Some(content.clone()),
        (None, Some(path)) => Some(fs::read_to_string(path).with_context(|| {
            format!("could not read namespace content from `{}`", path.display())
        })?),
        (None, None) => None,
        (Some(_), Some(_)) => bail!("--update and --update-file cannot be used together"),
    };
    if matches!(arguments.delete.as_deref(), Some("")) {
        bail!("KEY cannot be empty");
    }
    if matches!(arguments.title.as_deref(), Some("")) {
        bail!("title cannot be empty");
    }
    let client = configured_client()?;
    match (
        arguments.env.as_deref(),
        arguments.cluster.as_deref(),
        arguments.namespace.as_deref(),
    ) {
        (None, None, None) => {
            let environments = client.nav_tree(&arguments.app_id).await?;
            let view = AppOutput {
                app: &arguments.app_id,
                envs: environments
                    .into_iter()
                    .map(|environment| AppEnvironment {
                        env: environment.env,
                        clusters: environment
                            .clusters
                            .into_iter()
                            .map(|cluster| cluster.name)
                            .collect(),
                    })
                    .collect(),
            };
            if arguments.json {
                output::json(&view)
            } else {
                output::app(&view)
            }
        }
        (Some(env), Some(cluster), None) => {
            let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
            let view = NamespacesOutput {
                app: &arguments.app_id,
                env,
                cluster,
                namespaces: namespaces
                    .iter()
                    .map(|namespace| NamespaceOutput {
                        namespace: namespace.base_info.namespace_name.clone(),
                        format: namespace.format.clone().unwrap_or_default(),
                    })
                    .collect(),
            };
            if arguments.json {
                output::json(&view)
            } else {
                output::namespaces(&view)
            }
        }
        (Some(env), Some(cluster), Some(namespace)) => {
            if let Some(change) = &change {
                set_item(&client, &arguments, change).await
            } else if let Some(content) = &update {
                update_namespace(&client, &arguments, content).await
            } else if let Some(key) = &arguments.delete {
                delete_item(&client, &arguments, key).await
            } else if arguments.release {
                release_namespace(&client, &arguments).await
            } else {
                list_items(&client, &arguments, namespace, env, cluster).await
            }
        }
        _ => bail!("expected `apollo app APP [ENV CLUSTER [NAMESPACE]]`"),
    }
}

fn configured_client() -> Result<ApolloClient> {
    let config = ConfigStore::discover()?
        .load()?
        .context("Apollo URL is not configured; run `apollo login` first")?;
    let client = ApolloClient::new(&config.url)?;
    let session = config
        .session
        .as_ref()
        .ok_or(ApolloError::NotAuthenticated)?;
    client.with_session(session)
}

async fn list_items(
    client: &ApolloClient,
    arguments: &AppArgs,
    namespace: &str,
    env: &str,
    cluster: &str,
) -> Result<()> {
    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    let namespace = namespaces
        .iter()
        .find(|candidate| candidate.base_info.namespace_name == namespace)
        .with_context(|| format!("namespace `{namespace}` was not found"))?;
    let view = ContentOutput {
        app: &arguments.app_id,
        env,
        cluster,
        namespace: &namespace.base_info.namespace_name,
        content: crate::render::namespace_content(namespace)?,
    };
    if arguments.json {
        output::json(&view)
    } else if arguments.meta {
        output::namespace(&view, namespace.format.as_deref().unwrap_or_default())
    } else {
        output::raw_namespace(&view)
    }
}

async fn set_item(client: &ApolloClient, arguments: &AppArgs, change: &KeyValue) -> Result<()> {
    let env = arguments.env.as_deref().context("ENV is required")?;
    let cluster = arguments
        .cluster
        .as_deref()
        .context("CLUSTER is required")?;
    let namespace = arguments
        .namespace
        .as_deref()
        .context("NAMESPACE is required")?;
    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    require_properties(find_namespace(&namespaces, namespace)?, "--set")?;
    let item = find_item(&namespaces, namespace, &change.key)?;
    let is_update = item.is_some();

    let mut view = SetOutput {
        app: &arguments.app_id,
        env,
        cluster,
        namespace,
        operation: if is_update { "update" } else { "create" },
        key: &change.key,
        show_values: arguments.show_values,
        old_value: item.as_ref().and_then(|item| item.value.clone()),
        new_value: Some(&change.value),
        result: None,
    };
    output::set_preview(&view)?;
    if !arguments.yes && !prompt::confirm("Apply this change?")? {
        view.result = Some("aborted".to_owned());
        return output::set(&view);
    }

    if let Some(item) = item {
        let mut updated = item;
        updated.value = Some(change.value.clone());
        client
            .update_item(&arguments.app_id, env, cluster, namespace, &updated)
            .await?;
    } else {
        client
            .create_item(
                &arguments.app_id,
                env,
                cluster,
                namespace,
                &change.key,
                &change.value,
            )
            .await?;
    }

    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    let actual = find_item(&namespaces, namespace, &change.key)?.context(
        "the mutation response succeeded, but the item was missing on read-back; not retrying",
    )?;
    if actual.value.as_deref() != Some(change.value.as_str()) {
        bail!(
            "the mutation response succeeded, but the read-back value did not match; not retrying"
        );
    }
    view.result = Some(if is_update { "updated" } else { "created" }.to_owned());
    output::set(&view)
}

async fn update_namespace(client: &ApolloClient, arguments: &AppArgs, content: &str) -> Result<()> {
    let env = arguments.env.as_deref().context("ENV is required")?;
    let cluster = arguments
        .cluster
        .as_deref()
        .context("CLUSTER is required")?;
    let namespace = arguments
        .namespace
        .as_deref()
        .context("NAMESPACE is required")?;
    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    let namespace = find_namespace(&namespaces, namespace)?;
    let format = namespace.format.as_deref().unwrap_or_default();
    let old_content = crate::render::namespace_content(namespace)?;
    let normalized = if format.eq_ignore_ascii_case("properties") {
        content.to_owned()
    } else if format.eq_ignore_ascii_case("json") {
        crate::render::format_json(content)?
    } else {
        bail!("--update only supports properties and json namespaces");
    };

    let mut view = NamespaceUpdateOutput {
        app: &arguments.app_id,
        env,
        cluster,
        namespace: &namespace.base_info.namespace_name,
        operation: "update",
        format,
        show_values: arguments.show_values,
        old_value: arguments.show_values.then_some(old_content.as_str()),
        new_value: arguments.show_values.then_some(normalized.as_str()),
        result: None,
    };
    output::namespace_update_preview(&view)?;
    if !arguments.yes && !prompt::confirm("Apply this change?")? {
        view.result = Some("aborted".to_owned());
        return output::namespace_update(&view);
    }

    client
        .update_namespace(
            &arguments.app_id,
            env,
            cluster,
            namespace,
            format,
            &normalized,
        )
        .await?;
    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    let actual = find_namespace(&namespaces, &namespace.base_info.namespace_name)?;
    let actual_content = crate::render::namespace_content(actual)?;
    if actual_content.trim_end_matches(['\r', '\n']) != normalized.trim_end_matches(['\r', '\n']) {
        bail!("the namespace update succeeded, but the read-back content differed; not retrying");
    }
    view.result = Some("updated".to_owned());
    output::namespace_update(&view)
}

async fn delete_item(client: &ApolloClient, arguments: &AppArgs, key: &str) -> Result<()> {
    let env = arguments.env.as_deref().context("ENV is required")?;
    let cluster = arguments
        .cluster
        .as_deref()
        .context("CLUSTER is required")?;
    let namespace = arguments
        .namespace
        .as_deref()
        .context("NAMESPACE is required")?;
    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    require_properties(find_namespace(&namespaces, namespace)?, "--delete")?;
    let item = find_item(&namespaces, namespace, key)?
        .with_context(|| format!("configuration key `{key}` was not found; nothing to delete"))?;
    let mut view = SetOutput {
        app: &arguments.app_id,
        env,
        cluster,
        namespace,
        operation: "delete",
        key,
        show_values: arguments.show_values,
        old_value: item.value.clone(),
        new_value: None,
        result: None,
    };
    output::set_preview(&view)?;
    if !arguments.yes && !prompt::confirm("Apply this change?")? {
        view.result = Some("aborted".to_owned());
        return output::set(&view);
    }

    client
        .delete_item(&arguments.app_id, env, cluster, namespace, item.id)
        .await?;
    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    if find_item(&namespaces, namespace, key)?.is_some() {
        bail!("the delete response succeeded, but the key was still present; not retrying");
    }
    view.result = Some("deleted".to_owned());
    output::set(&view)
}

async fn release_namespace(client: &ApolloClient, arguments: &AppArgs) -> Result<()> {
    let env = arguments.env.as_deref().context("ENV is required")?;
    let cluster = arguments
        .cluster
        .as_deref()
        .context("CLUSTER is required")?;
    let namespace = arguments
        .namespace
        .as_deref()
        .context("NAMESPACE is required")?;
    let namespaces = client.namespaces(&arguments.app_id, env, cluster).await?;
    find_namespace(&namespaces, namespace)?;
    let title = arguments.title.clone().unwrap_or_else(|| {
        chrono::Local::now()
            .format("%Y%m%d%H%M%S-cli-release")
            .to_string()
    });
    let comment = arguments.comment.as_deref().unwrap_or_default();
    let mut view = ReleaseOutput {
        app: &arguments.app_id,
        env,
        cluster,
        namespace,
        title: &title,
        comment,
        result: None,
    };
    output::release_preview(&view)?;
    if !arguments.yes && !prompt::confirm("Apply this change?")? {
        view.result = Some("aborted".to_owned());
        return output::release(&view);
    }

    client
        .release_namespace(&arguments.app_id, env, cluster, namespace, &title, comment)
        .await?;
    view.result = Some("released".to_owned());
    output::release(&view)
}

fn find_namespace<'a>(namespaces: &'a [Namespace], namespace: &str) -> Result<&'a Namespace> {
    namespaces
        .iter()
        .find(|candidate| candidate.base_info.namespace_name == namespace)
        .with_context(|| format!("namespace `{namespace}` was not found"))
}

fn require_properties(namespace: &Namespace, operation: &str) -> Result<()> {
    if !namespace
        .format
        .as_deref()
        .is_some_and(|format| format.eq_ignore_ascii_case("properties"))
    {
        bail!("{operation} is only supported for properties namespaces");
    }
    Ok(())
}

fn find_item(namespaces: &[Namespace], namespace: &str, key: &str) -> Result<Option<Item>> {
    let namespace = namespaces
        .iter()
        .find(|candidate| candidate.base_info.namespace_name == namespace)
        .with_context(|| format!("namespace `{namespace}` was not found"))?;
    let mut matches = namespace
        .items
        .iter()
        .filter(|entry| !entry.deleted && entry.item.key == key)
        .map(|entry| entry.item.clone());
    let item = matches.next();
    if matches.next().is_some() {
        bail!("configuration key `{key}` occurs more than once; refusing an ambiguous set");
    }
    Ok(item)
}
