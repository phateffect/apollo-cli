---
name: apollo-cli
description: 使用 apollo CLI 安全操作 Apollo 配置中心：检查登录状态、搜索 app、定位 environment/cluster/namespace、读取配置、增删改单个 properties key、全量更新 properties 或 json namespace，以及发布 namespace。用户提到 Apollo 配置中心、apollo 命令、app id、环境、集群、namespace、线上配置排查、配置修改或发布时应使用本技能；即使只给出 app/env/cluster/namespace 四段路径，也先核对层级、格式、敏感信息和写入授权。
---

# Apollo CLI

用 `apollo` 操作 Apollo 配置中心。把定位、修改、发布视为三个独立阶段：先确认准确目标，再执行用户明确授权的修改；只有用户另行授权时才发布。

## 安全边界

- 不读取或输出 Cookie、token、密码等凭据。
- 登录时让用户通过隐藏提示输入密码；不要接收、保存或传递密码。
- 配置可能包含密码、密钥、token 或内部地址。只读取和展示完成任务所需的最少内容，不要把真实秘密写进工具调用、回复、持久化内容或日志。
- 如果敏感值已经保存在受保护的 UTF-8 文件中，使用 `--set-file` 或 `--update-file`，不要读取或展示文件内容。否则给出使用占位符的命令，让用户在受控终端完成。
- 修改和发布都是外部写操作，且授权彼此独立。“修改配置”不等于“发布配置”。
- 写入结果不明确时先重新读取，绝不盲目重试。

## 验证登录状态

先执行：

```sh
apollo status
```

- 已登录时继续，并核对目标 Apollo 地址和当前用户。
- 未配置或登录失效时，让用户执行 `apollo login`。已知地址或用户名时可提供 `--url`、`--username`，密码仍由用户交互输入。
- 网络、证书或服务端错误应原样报告并停止，不要误判为未登录。

需要严格提取字段时使用：

```sh
apollo status --json
```

## 逐级定位

不要猜测 app、env、cluster 或 namespace。默认使用简洁的普通输出：

```sh
# 搜索全部 app，或按关键词搜索
apollo search
apollo search query

# 确认 environment 和 cluster
apollo app app_id

# 确认 namespace 完整名称和 format
apollo app app_id env cluster

# 读取 namespace 内容
apollo app app_id env cluster namespace
```

namespace 名称按完整字符串处理，例如 `jwks.json` 中的点不会拆分。

需要同时保留目标信息和内容时使用：

```sh
apollo app app_id env cluster namespace --meta
```

直接路径也可用于只读查询：

```sh
apollo app_id env cluster namespace
```

普通输出已经适合阅读和逐级导航。仅在需要程序化处理、严格字段提取或普通输出存在歧义时添加 `--json`：

```sh
apollo search query --json
apollo app app_id --json
apollo app app_id env cluster --json
apollo app app_id env cluster namespace --json
```

## 修改单个 properties key

先读取目标 namespace，确认其 format 为 `properties`，并检查 key 的当前状态：

```sh
apollo app app_id env cluster namespace --set key value

# value 含敏感信息时从受保护文件读取
apollo app app_id env cluster namespace --set-file key /secure/value.txt
```

- key 存在时更新，不存在时创建。
- key 为空或存在重复项时停止，不要绕过保护。
- 删除前确认 key 存在：

```sh
apollo app app_id env cluster namespace --delete key
```

不要把“删除 key”扩大为删除 namespace 或其他配置。

## 全量更新 namespace

```sh
apollo app app_id env cluster namespace --update content

# 完整内容含敏感信息时从受保护文件读取
apollo app app_id env cluster namespace --update-file /secure/namespace.txt
```

- 仅对 `properties` 和 `json` 使用全量更新。
- 这是完整内容替换，不是局部补丁。先读取现有内容，确认新内容保留了所有仍需存在的配置。
- 写入后重新读取并核对关键配置；不一致时报告当前状态，不要自动重试。

示例只使用非敏感值：

```sh
apollo app sample-app dev default application --update 'dsn = example-value'
apollo app sample-app dev default jwks.json --update '{"X":"Y"}'
```

## 发布 namespace

发布前重新读取目标 namespace，确认 app、env、cluster、namespace 和待发布内容都正确：

```sh
apollo app app_id env cluster namespace --release

apollo app app_id env cluster namespace \
  --release --title 'title' --comment 'comment'
```

不要在修改成功后自动发布，也不要把一次发布失败理解为可以重复发布。

## 确认与结果处理

- 默认保留交互确认，让用户核对目标和操作类型。
- 写入预览默认隐藏配置值；不要为了查看敏感内容添加 `--show-values`。
- 只有用户已明确授权同一目标、最终内容和操作类型，且内容不敏感时，才在非交互执行中添加 `--yes`。
- 用户取消操作时，不要报告为失败或完成。
- 写入返回成功后仍要重新读取并核对；结果不一致时说明配置可能已经发生变化，并停止重试。
- 权限不足、登录失效、namespace 不存在或格式不支持时，报告具体错误，不要改用其他接口绕过限制。

## 标准流程

### 查询

1. 用 `status` 验证登录状态。
2. 用 `search` 和 `app` 命令逐级定位。
3. 读取 namespace，只汇报完成任务所需的内容。
4. 需要程序化处理时再使用 `--json`。

### 修改

1. 完成只读定位，确认 format，并记录当前状态。
2. 确认用户授权的目标、操作类型和最终值；敏感内容交还用户处理。
3. 执行一种写操作并保留确认；满足非交互条件时才使用 `--yes`。
4. 重新读取并核对。结果不明确时报告“可能已写入”，不要重试。
5. 只有用户另行授权时才进入发布流程。

### 发布

1. 重新读取并确认目标 namespace 和待发布内容。
2. 确认用户明确授权发布；需要时收集 title 和 comment。
3. 执行一次发布并报告结果，不自动重复。
