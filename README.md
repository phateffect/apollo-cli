# apollo

用于操作 Apollo 配置中心的 Rust CLI。支持登录、搜索 app、逐级浏览环境与 namespace，以及新增或修改配置项。

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

登录成功后会安全保存登录状态；用户名和密码不会写入配置。

可通过 `APOLLO_CONFIG_DIR` 指定另一个配置目录，例如隔离测试环境：

```sh
APOLLO_CONFIG_DIR=/tmp/apollo-cli-config apollo status
```

```sh
apollo status
apollo status --json
```

`status` 显示配置的 URL、登录状态和当前用户名。需要程序化处理时使用 `--json`。

## 搜索 app

```sh
# 分页获取全部 app
apollo search

# 分页获取关键词的全部搜索结果
apollo search keyword

# 输出 JSON
apollo search keyword --json
```

普通搜索输出为纯 frontmatter，在 `apps` 列表中逐项展示 `appId` 和 `name`，无正文。

## 逐级浏览配置

```sh
# 列出 app 的 environment 和 cluster
apollo app APP_ID

# 列出 cluster 下的 namespace 和格式
apollo app APP_ID DEV default

# 仅输出 namespace 内容，可使用直接路径
apollo APP_ID DEV default application

# 等价的 app 子命令形式
apollo app APP_ID DEV default application

# 输出 frontmatter 和 Markdown 元信息及内容
apollo app APP_ID DEV default application --meta

# namespace 使用完整名称，名称中的点不会被拆分
apollo APP_ID DEV default jwks.json

# 输出包含定位字段和 content 的 JSON
apollo app APP_ID DEV default application --json
```

### 普通输出与 JSON

- app 和 cluster 层级的普通输出适合直接阅读和逐级导航。
- namespace 层级默认仅输出配置内容。
- `--meta` 同时输出 app、env、cluster、namespace 等定位信息和配置内容。
- `--json` 用于程序化处理，包含定位字段和 `content`。

### namespace 展示格式

namespace 内容会根据 format 以适合阅读的形式展示：

- `json` 和 `properties` 会格式化后输出；无效 JSON 会明确报错。
- 其他格式以表格展示，仅支持读取。

## 新增、修改与删除配置值

```sh
# properties：key 存在时更新，不存在时新增
apollo app APP_ID DEV default application --set dsn 'new-value'

# 从 UTF-8 文件读取 value，避免敏感值出现在命令行参数中
apollo app APP_ID DEV default application --set-file dsn /secure/value.txt

# properties：全量替换 namespace 内容
apollo app APP_ID DEV default application --update 'dsn = new-value'

# 从 UTF-8 文件读取完整 namespace 内容
apollo app APP_ID DEV default application --update-file /secure/application.properties

# json：全量替换，发送前格式化为两空格缩进且不转义非 ASCII 字符
apollo app APP_ID DEV default jwks.json --update '{"X":"Y"}'

# properties：删除已有 key
apollo app APP_ID DEV default application --delete dsn

# 所有写操作默认要求确认，明确授权时可用 --yes 跳过
apollo app APP_ID DEV default application --delete dsn --yes
```

`--set`、`--set-file` 和 `--delete` 仅支持 properties。`--update`、`--update-file` 支持 properties 和 json；properties 原样发送，JSON 在发送前解析并格式化。文件输入按 UTF-8 原样读取。所有配置写操作都会重新读取目标 namespace 并核对结果，异常时不自动重试。

写操作的确认预览、取消结果和完成结果均使用 frontmatter 展示目标、操作类型及 `result`。配置值默认显示为 `redacted`；仅在内容不敏感且确实需要时使用 `--show-values` 显示字符串形式的 `oldValue` 和 `newValue`。

## 发布 namespace

```sh
# 自动生成 YYYYMMDDHHmmss-cli-release 标题，comment 默认为空字符串
apollo app APP_ID DEV default application --release

# 显式指定标题和 comment
apollo app APP_ID DEV default application \
  --release --title 'manual-release' --comment 'release note'

# 明确授权时跳过确认
apollo app APP_ID DEV default application --release --yes
```

发布前会先确认目标 namespace 存在；目标不存在时不会发送发布请求。

## usage 规范

CLI 可以生成 usage 规范，用于创建 shell 补全或命令文档：

```sh
apollo __usage_spec__ > apollo.usage.kdl
usage generate completion bash apollo --file apollo.usage.kdl
```

也可以使用 `usage` 根据该规范生成其他 shell 的补全或命令文档。

## 自动发布

推送 `YYYY.MM.DD` 格式的 tag 时，GitHub Actions 会自动同步 Cargo package version、验证项目并发布三个平台的二进制：

- Linux x86_64：`.tar.gz`
- Windows x86_64：`.zip`
- macOS x86_64：`.tar.gz`

例如：

```sh
git tag 2026.11.22
git push origin 2026.11.22
```

只有符合 `YYYY.MM.DD` 的有效日期 tag 会触发发布；合并或推送 `main` 不会创建 release。发布产物中的 `apollo --version` 与 tag 保持一致。
