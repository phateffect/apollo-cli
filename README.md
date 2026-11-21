# apollo

用于操作 Apollo 配置中心的 Rust CLI。支持登录、搜索 App、逐级浏览环境与 Namespace，以及新增或修改配置项。

## 开发环境

使用 mise 安装 Rust 工具链：

```sh
mise install
```

常用任务：

```sh
mise run fmt-check
mise run lint
mise run test
mise run build
```

构建后的二进制位于 `target/release/apollo`。开发时也可以使用 `cargo run -- <命令>`。

## 登录与状态

```sh
apollo login --url "$APOLLO_URL" --username "$APOLLO_USERNAME"
```

省略 `--url` 或 `--username` 时会交互询问。密码始终单独提示并在终端隐藏输入，不提供 `--password` 参数。

登录成功后，CLI 将 URL 和会话 Cookie 保存到用户配置目录中的 `apollo/session.json`。Unix 上目录权限为 `0700`，文件权限为 `0600`。用户名和密码不会写入该文件。

可通过 `APOLLO_CONFIG_DIR` 指定另一个配置目录，例如隔离测试环境：

```sh
APOLLO_CONFIG_DIR=/tmp/apollo-cli-config apollo status
```

```sh
apollo status
apollo status --json
```

`status` 显示配置的 URL。存在有效登录态时，它向 Apollo 请求当前用户信息并显示用户名；用户名不是从本地文件读取的。`login` 和 `status` 的普通输出均使用 frontmatter 展示 `url`、`loggedIn` 和 `username`，JSON 结构保持不变。

## 搜索 App

```sh
# 分页获取全部 App
apollo search

# 分页获取关键词的全部搜索结果
apollo search keyword

# 输出 JSON
apollo search keyword --json
```

普通搜索输出为纯 frontmatter，在 `apps` 列表中逐项展示 `appId` 和 `name`，无正文。

## 逐级浏览配置

```sh
# 列出 App 的 environment 和 cluster
apollo app APP_ID

# 列出 Cluster 下的 Namespace 和格式
apollo app APP_ID DEV default

# 仅输出 Namespace 内容，可使用直接路径
apollo APP_ID DEV default application

# 等价的 app 子命令形式
apollo app APP_ID DEV default application

# 输出 frontmatter 和 Markdown 元信息及内容
apollo app APP_ID DEV default application --meta

# Namespace 使用完整名称，名称中的点不会被拆分
apollo APP_ID DEV default jwks.json

# 输出包含定位字段和 content 的 JSON
apollo app APP_ID DEV default application --json
```

### 普通输出与 JSON

- App 和 Cluster 层级使用纯 frontmatter，将 `envs`、`namespaces` 嵌入其中，无正文。
- Namespace 层级默认仅输出 `content` 本身，不带 frontmatter 或代码块。
- `--meta` 输出当前 frontmatter 和 Markdown：回显 `app`、`env`、`cluster`、`namespace`，并将内容放入对应语言的 fenced code block。
- `--json` 保留定位字段，并将相同渲染结果放入 `content` 字符串。

### Namespace 展示格式

Namespace 的 `content` 按 `format` 选择渲染方式：

- `json`：解析配置内容后通过 MiniJinja 的 `tojson(indent=2)` 输出；中文等非 ASCII 字符保持原样。无效 JSON 会明确报错。
- `properties`：按 `lineNum` 稳定排序，通过 MiniJinja 模板逐行输出 `key = value # comment`；没有 key 的 comment 单独占一行。
- 其他格式：保持 `NAMESPACE`、`KEY`、`VALUE` 表格输出，仅支持读取。

服务端返回的 `isDeleted=true` item 会在展示、单 key 查找和写后回读时统一忽略。

## 新增、修改与删除配置值

```sh
# properties：key 存在时更新，不存在时新增
apollo app APP_ID DEV default application --set dsn 'new-value'

# 从 UTF-8 文件读取 value，避免敏感值出现在命令行参数中
apollo app APP_ID DEV default application --set-file dsn /secure/value.txt

# properties：全量替换 Namespace 内容
apollo app APP_ID DEV default application --update 'dsn = new-value'

# 从 UTF-8 文件读取完整 Namespace 内容
apollo app APP_ID DEV default application --update-file /secure/application.properties

# json：全量替换，发送前格式化为两空格缩进且不转义非 ASCII 字符
apollo app APP_ID DEV default jwks.json --update '{"X":"Y"}'

# properties：删除已有 key
apollo app APP_ID DEV default application --delete dsn

# 所有写操作默认要求确认，明确授权时可用 --yes 跳过
apollo app APP_ID DEV default application --delete dsn --yes
```

`--set`、`--set-file` 和 `--delete` 仅支持 properties。`--update`、`--update-file` 支持 properties 和 json；properties 原样发送，JSON 在发送前解析并格式化。文件输入按 UTF-8 原样读取。所有配置写操作都会重新读取目标 Namespace 并核对结果，异常时不自动重试。

写操作的确认预览、取消结果和完成结果均使用 frontmatter 展示目标、操作类型及 `result`。配置值默认显示为 `redacted`；全量更新会展示更新前后的字节数。仅在内容不敏感且确实需要时使用 `--show-values` 显示值。

## 发布 Namespace

```sh
# 自动生成 YYYYMMDDHHmmss-cli-release 标题，comment 默认为空字符串
apollo app APP_ID DEV default application --release

# 显式指定标题和 comment
apollo app APP_ID DEV default application \
  --release --title 'manual-release' --comment 'release note'

# 明确授权时跳过确认
apollo app APP_ID DEV default application --release --yes
```

发布请求使用 `POST .../releases`，包含 `releaseTitle`、`releaseComment` 和固定的 `isEmergencyPublish=false`。自动标题只生成一次，预览和实际请求使用相同标题。发布预览、取消和完成结果使用 frontmatter 展示。

发布前会先确认目标 Namespace 存在；目标不存在时不会发送发布请求。

## usage 规范

CLI 使用 clap 解析参数，并通过 `clap_usage` 从同一棵命令树生成 usage 规范：

```sh
apollo __usage_spec__ > apollo.usage.kdl
usage generate completion bash apollo --file apollo.usage.kdl
```

也可以使用 `usage` 根据该规范生成其他 shell 的补全或命令文档。
