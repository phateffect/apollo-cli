use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "apollo",
    version,
    about = "Operate an Apollo configuration center",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Log in and cache the session")]
    Login(LoginArgs),
    #[command(about = "Show the configured URL and login state")]
    Status(OutputArgs),
    #[command(about = "Search apps and retrieve every result page")]
    Search(SearchArgs),
    #[command(about = "Inspect an app's environments, namespaces, and configuration items")]
    App(Box<AppArgs>),
    #[command(name = "__usage_spec__", hide = true)]
    UsageSpec,
    #[command(external_subcommand)]
    Direct(Vec<OsString>),
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    #[arg(long, value_name = "USERNAME")]
    pub username: Option<String>,
}

#[derive(Debug, Args)]
pub struct OutputArgs {
    #[arg(long, help = "Print machine-readable JSON")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("mutation")
        .args(["set", "set_file", "update", "update_file", "delete", "release"])
        .multiple(false)
))]
#[command(group(
    ArgGroup::new("value_mutation")
        .args(["set", "set_file", "update", "update_file", "delete"])
        .multiple(false)
))]
pub struct AppArgs {
    #[arg(value_name = "APP_ID")]
    pub app_id: String,
    #[arg(value_name = "ENV", requires = "cluster")]
    pub env: Option<String>,
    #[arg(value_name = "CLUSTER")]
    pub cluster: Option<String>,
    #[arg(value_name = "NAMESPACE", requires = "cluster")]
    pub namespace: Option<String>,
    #[arg(
        long,
        help = "Explicitly list environments or namespaces",
        conflicts_with = "namespace"
    )]
    pub list: bool,
    #[arg(long, help = "Show frontmatter and Markdown instead of raw content", requires = "namespace", conflicts_with_all = ["list", "set", "update", "delete", "release", "json"])]
    pub meta: bool,
    #[arg(long, num_args = 2, value_names = ["KEY", "VALUE"], help = "Create or update a non-sensitive properties key", requires = "namespace", conflicts_with_all = ["list", "set_file", "update", "update_file", "delete", "meta"])]
    pub set: Option<Vec<String>>,
    #[arg(long, num_args = 2, value_names = ["KEY", "FILE"], help = "Create or update a properties key from a UTF-8 file", requires = "namespace", conflicts_with_all = ["list", "set", "update", "update_file", "delete", "meta"])]
    pub set_file: Option<Vec<String>>,
    #[arg(long, value_name = "CONTENT", help = "Replace the entire namespace with non-sensitive content", requires = "namespace", conflicts_with_all = ["list", "set", "set_file", "update_file", "delete", "meta", "json"])]
    pub update: Option<String>,
    #[arg(long, value_name = "FILE", help = "Replace the entire namespace from a UTF-8 file", requires = "namespace", conflicts_with_all = ["list", "set", "set_file", "update", "delete", "meta", "json"])]
    pub update_file: Option<PathBuf>,
    #[arg(long, value_name = "KEY", help = "Delete a properties key", requires = "namespace", conflicts_with_all = ["list", "set", "set_file", "update", "update_file", "meta", "json"])]
    pub delete: Option<String>,
    #[arg(
        long,
        help = "Apply the change without an interactive confirmation",
        requires = "mutation"
    )]
    pub yes: bool,
    #[arg(
        long,
        help = "Show configuration values in mutation previews and results",
        requires = "value_mutation",
        conflicts_with = "release"
    )]
    pub show_values: bool,
    #[arg(long, help = "Publish the namespace", requires = "namespace", conflicts_with_all = ["list", "meta", "json"])]
    pub release: bool,
    #[arg(long, value_name = "TITLE", requires = "release")]
    pub title: Option<String>,
    #[arg(long, value_name = "COMMENT", requires = "release")]
    pub comment: Option<String>,
    #[arg(long, conflicts_with_all = ["set", "set_file", "update", "update_file", "delete", "release", "meta"])]
    pub json: bool,
}

pub fn usage_spec() -> String {
    let mut command = Cli::command();
    let mut buffer = Vec::new();
    clap_usage::generate(&mut command, "apollo", &mut buffer);
    String::from_utf8(buffer).expect("clap_usage emits UTF-8")
}
