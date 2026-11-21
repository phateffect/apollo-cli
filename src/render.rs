use anyhow::{Context, Result};
use minijinja::value::Serde;
use minijinja::{Environment, context};
use serde::Serialize;

use crate::model::{App, AppOutput, ContentOutput, Namespace, NamespacesOutput};

const SEARCH_TEMPLATE: &str = concat!(
    "---\n",
    "apps:",
    "{% if data|length == 0 %} []{% endif %}",
    "\n",
    "{% for app in data %}",
    " - appId: {{ app.appId }}\n",
    "   name: {{ app.name }}\n",
    "{% endfor %}",
    "---",
);
const STATUS_TEMPLATE: &str = concat!(
    "---\n",
    "url: ",
    "{% if data.url %}{{ data.url }}{% else %}null{% endif %}",
    "\n",
    "loggedIn: {{ data.loggedIn|tojson }}\n",
    "username: ",
    "{% if data.username %}{{ data.username }}{% else %}null{% endif %}",
    "\n",
    "---",
);
const SET_TEMPLATE: &str = concat!(
    "---\n",
    "app: {{ data.app }}\n",
    "env: {{ data.env }}\n",
    "cluster: {{ data.cluster }}\n",
    "namespace: {{ data.namespace }}\n",
    "operation: {{ data.operation }}\n",
    "key: {{ data.key }}\n",
    "{% if data.showValues %}",
    "oldValue: ",
    "{% if data.oldValue is none %}null{% else %}{{ data.oldValue|tojson }}{% endif %}",
    "\n",
    "newValue: ",
    "{% if data.newValue is none %}null{% else %}{{ data.newValue|tojson }}{% endif %}",
    "\n",
    "{% else %}values: redacted\n{% endif %}",
    "{% if data.result %}result: {{ data.result }}\n{% endif %}",
    "---",
);
const NAMESPACE_UPDATE_TEMPLATE: &str = concat!(
    "---\n",
    "app: {{ data.app }}\n",
    "env: {{ data.env }}\n",
    "cluster: {{ data.cluster }}\n",
    "namespace: {{ data.namespace }}\n",
    "operation: {{ data.operation }}\n",
    "format: {{ data.format }}\n",
    "{% if data.showValues %}",
    "oldValue: {{ data.oldValue|tojson }}\n",
    "newValue: {{ data.newValue|tojson }}\n",
    "{% else %}values: redacted\n{% endif %}",
    "{% if data.result %}result: {{ data.result }}\n{% endif %}",
    "---",
);
const RELEASE_TEMPLATE: &str = concat!(
    "---\n",
    "app: {{ data.app }}\n",
    "env: {{ data.env }}\n",
    "cluster: {{ data.cluster }}\n",
    "namespace: {{ data.namespace }}\n",
    "operation: release\n",
    "title: {{ data.title }}\n",
    "comment: {{ data.comment }}\n",
    "{% if data.result %}result: {{ data.result }}\n{% endif %}",
    "---",
);
const APP_TEMPLATE: &str = concat!(
    "---\n",
    "app: {{ data.app }}\n",
    "envs:\n",
    "{% for environment in data.envs %}",
    " - name: {{ environment.env }}\n",
    "   clusters: [",
    "{% for cluster in environment.clusters %}",
    "{{ cluster }}{% if not loop.last %}, {% endif %}",
    "{% endfor %}]\n",
    "{% endfor %}",
    "---",
);
const NAMESPACES_TEMPLATE: &str = concat!(
    "---\n",
    "app: {{ data.app }}\n",
    "env: {{ data.env }}\n",
    "cluster: {{ data.cluster }}\n",
    "namespaces:\n",
    "{% for namespace in data.namespaces %}",
    "  - name: {{ namespace.namespace }}\n",
    "    format: {{ namespace.format }}\n",
    "{% endfor %}",
    "---",
);
const CONTENT_TEMPLATE: &str = concat!(
    "---\n",
    "app: {{ data.app }}\n",
    "env: {{ data.env }}\n",
    "cluster: {{ data.cluster }}\n",
    "namespace: {{ data.namespace }}\n",
    "---\n\n",
    "{{ fence }}{{ language }}\n",
    "{{ data.content }}",
    "{{ fence }}\n",
);
const JSON_TEMPLATE: &str = "{{ content|tojson(indent=2) }}";
const PROPERTIES_TEMPLATE: &str = concat!(
    "{% for item in items %}",
    "{% if item.key %}",
    "{{ item.key }} = {{ item.value }}",
    "{% if item.comment %} # {{ item.comment }} {% endif %}",
    "{% elif item.comment %}",
    "{{ item.comment }}",
    "{% endif %}",
    "\n",
    "{% endfor %}",
);
const TABLE_TEMPLATE: &str = concat!(
    "NAMESPACE\tKEY\tVALUE\n",
    "{% for item in items %}",
    "{{ item.namespace }}\t{{ item.key }}\t{{ item.value }}\n",
    "{% endfor %}",
);

#[derive(Serialize)]
struct PropertyItem<'a> {
    key: &'a str,
    value: &'a str,
    comment: &'a str,
}

#[derive(Serialize)]
struct TableItem {
    namespace: String,
    key: String,
    value: String,
}

pub fn search(apps: &[App]) -> Result<String> {
    render_view(SEARCH_TEMPLATE, apps)
}

pub fn status<T: Serialize>(output: &T) -> Result<String> {
    render_view(STATUS_TEMPLATE, output)
}

pub fn set<T: Serialize>(output: &T) -> Result<String> {
    render_view(SET_TEMPLATE, output)
}

pub fn namespace_update<T: Serialize>(output: &T) -> Result<String> {
    render_view(NAMESPACE_UPDATE_TEMPLATE, output)
}

pub fn release<T: Serialize>(output: &T) -> Result<String> {
    render_view(RELEASE_TEMPLATE, output)
}

pub fn app(output: &AppOutput) -> Result<String> {
    render_view(APP_TEMPLATE, output)
}

pub fn namespaces(output: &NamespacesOutput) -> Result<String> {
    render_view(NAMESPACES_TEMPLATE, output)
}

pub fn content_output(output: &ContentOutput, format: &str) -> Result<String> {
    let language = if format.eq_ignore_ascii_case("json") {
        "json"
    } else if format.eq_ignore_ascii_case("properties") {
        "properties"
    } else {
        "text"
    };
    let fence = markdown_fence(&output.content);
    let rendered = Environment::new().render_str(
        CONTENT_TEMPLATE,
        context! { data => Serde(output), language => language, fence => fence },
    )?;
    Ok(format!("{rendered}\n"))
}

fn markdown_fence(content: &str) -> String {
    let longest = content
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    "`".repeat(longest.saturating_add(1).max(3))
}

pub fn namespace_content(namespace: &Namespace) -> Result<String> {
    match namespace.format.as_deref() {
        Some(format) if format.eq_ignore_ascii_case("json") => json(namespace),
        Some(format) if format.eq_ignore_ascii_case("properties") => properties(namespace),
        _ => table(namespace),
    }
}

fn render_view<T: Serialize + ?Sized>(template: &str, data: &T) -> Result<String> {
    let rendered = Environment::new().render_str(template, context! { data => Serde(data) })?;
    Ok(format!("{rendered}\n"))
}

fn json(namespace: &Namespace) -> Result<String> {
    let items = namespace
        .items
        .iter()
        .filter(|entry| !entry.deleted)
        .collect::<Vec<_>>();
    let entry = items
        .iter()
        .find(|entry| entry.item.key == "content")
        .or_else(|| {
            if items.len() == 1 {
                items.first()
            } else {
                None
            }
        })
        .with_context(|| {
            format!(
                "JSON namespace `{}` must contain a content item",
                namespace.base_info.namespace_name
            )
        })?;
    let content = entry.item.value.as_deref().unwrap_or_default();
    let rendered = format_json(content).with_context(|| {
        format!(
            "namespace `{}` contains invalid JSON",
            namespace.base_info.namespace_name
        )
    })?;
    Ok(format!("{rendered}\n"))
}

pub fn format_json(content: &str) -> Result<String> {
    let content: serde_json::Value = serde_json::from_str(content).context("invalid JSON")?;
    Environment::new()
        .render_str(JSON_TEMPLATE, context! { content => Serde(content) })
        .map_err(Into::into)
}

fn properties(namespace: &Namespace) -> Result<String> {
    let mut entries = namespace
        .items
        .iter()
        .filter(|entry| !entry.deleted)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.item.line_num);
    let items = entries
        .into_iter()
        .map(|entry| PropertyItem {
            key: &entry.item.key,
            value: entry.item.value.as_deref().unwrap_or_default(),
            comment: entry.item.comment.as_deref().unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    Environment::new()
        .render_str(PROPERTIES_TEMPLATE, context! { items => Serde(items) })
        .map_err(Into::into)
}

fn table(namespace: &Namespace) -> Result<String> {
    let items = namespace
        .items
        .iter()
        .filter(|entry| !entry.deleted)
        .map(|entry| TableItem {
            namespace: one_line(&namespace.base_info.namespace_name),
            key: one_line(&entry.item.key),
            value: one_line(entry.item.value.as_deref().unwrap_or_default()),
        })
        .collect::<Vec<_>>();
    Environment::new()
        .render_str(TABLE_TEMPLATE, context! { items => Serde(items) })
        .map_err(Into::into)
}

fn one_line(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
