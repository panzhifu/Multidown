# MultiDown

一个用 Rust 编写的多线程下载管理器，支持并发下载、断点续传、动态分片调整和实时进度显示。

## 架构亮点

- **全 actix actor 架构**：CLI、核心、UI、日志全部基于 actix actor 解耦，主流程无阻塞、易扩展。
- **统一错误类型**：全局错误处理统一为 `DownloadError`，参数、配置、IO、网络等全链路一致。
- **异步日志系统**：所有日志通过 LoggerActor 异步写入，支持动态日志级别调整。
- **配置与参数校验**：配置加载、参数解析、URL 校验等全部严格校验，保证主流程健壮。
- **模块化设计**：core、ui、cli、config、utils 各司其职，易于维护和扩展。

## 功能特点

### 核心功能
- **多文件并发下载**: 支持同时下载多个文件，每个文件独立管理
- **任务管理系统**: 完整的任务生命周期管理，支持暂停、恢复、取消
- **动态分片调整**: 根据网络速度自动调整并发分片数，优化下载性能
- **断点续传**: 支持下载中断后从断点继续下载
- **实时进度显示**: 统一的UI进度管理器，显示所有任务的下载进度
- **自动重试机制**: 网络错误时自动重试，提高下载成功率

### 技术特性
- **多协议支持**: HTTP/HTTPS/FTP
- **智能分片**: 大文件自动分片下载，小文件顺序下载
- **内存优化**: 使用流式下载，减少内存占用
- **异步架构**: 基于tokio和actix的异步运行时，高效处理并发
- **配置灵活**: 支持多种下载参数配置

### 用户体验
- **统一进度管理**: 所有下载任务在同一个UI界面显示
- **详细状态信息**: 显示下载速度、剩余时间、进度百分比
- **错误处理**: 友好的错误提示和日志记录
- **配置持久化**: 支持配置文件保存和加载

## 架构设计

### 模块结构
```
src/
├── core/
│   ├── actor_manager.rs   # 任务管理、调度、元数据
│   ├── actor_task.rs      # 单任务下载、分片、进度
│   └── error.rs           # 统一错误类型
├── ui/
│   ├── progress.rs        # 进度条管理、UI显示
│   └── mod.rs             # UI模块入口
├── config/
│   └── mod.rs             # 配置管理、校验
├── cli/
│   └── mod.rs             # 命令行接口
└── utils/
    ├── logger.rs          # actix异步日志
    └── validator.rs       # 参数/URL校验
```

### 日志与错误处理
- **LoggerActor**：所有日志通过 actix 消息异步写入文件，支持动态日志级别调整。
- **DownloadError**：全局统一错误类型，支持 IO、网络、参数、配置等多种错误分级。
- **配置与参数校验**：所有配置项、命令行参数、URL 均严格校验，主流程只处理已验证数据。

## 安装

确保你已经安装了 Rust 和 Cargo。然后运行：

```bash
git clone https://github.com/panzhifu/Multidown.git
cd multidown
cargo build --release
```

## 使用方法

### 基本用法

下载单个文件：
```bash
cargo run -- https://example.com/file.zip
```

下载多个文件：
```bash
cargo run -- https://example.com/file1.zip https://example.com/file2.zip
```

使用编译后的二进制文件：
```bash
./target/release/multidown https://example.com/file.zip
```

### 高级用法

指定输出目录：
```bash
cargo run -- --output ./downloads https://example.com/file.zip
```

设置并发数：
```bash
cargo run -- --concurrent 8 https://example.com/file.zip
```

### 控制命令

- `q` 或 `Esc`: 暂停下载并退出
- `Ctrl+C`: 强制退出
- 支持任务暂停/恢复/取消

## 配置

### 配置文件 (multidown.conf)

```toml
# 下载配置
[download]
concurrent_tasks = 10        # 并发任务数
chunk_size = 1048576         # 分片大小 (1MB)
max_chunks = 16              # 最大分片数
min_chunks = 1               # 最小分片数
timeout = 30                 # 超时时间(秒)
max_retries = 3              # 最大重试次数

# 网络配置
[network]
user_agent = "MultiDown/1.0" # 用户代理
max_redirects = 5            # 最大重定向次数
enable_proxy = false         # 是否启用代理
proxy_url = ""               # 代理URL

# 输出配置
[output]
default_directory = "./downloads"  # 默认下载目录
create_subdirectories = true       # 是否创建子目录
overwrite_existing = false         # 是否覆盖已存在文件
```

### 环境变量

```bash
export MULTIDOWN_CONFIG_PATH="/path/to/config.toml"
export MULTIDOWN_OUTPUT_DIR="/path/to/downloads"
```

## 开发

### 构建和测试

```bash
# 检查代码
cargo check

# 运行测试
cargo test

# 格式化代码
cargo fmt

# 代码检查
cargo clippy

# 构建发布版本
cargo build --release
```

### 开发环境

```bash
# 安装开发依赖
cargo install cargo-watch

# 监听文件变化并自动测试
cargo watch -x check -x test
```

## 性能特性

### 动态分片调整
- 根据网络速度自动调整并发分片数
- 高速网络：增加分片数提高并发
- 低速网络：减少分片数避免拥塞

### 断点续传
- 自动保存下载进度到 `.progress` 文件
- 支持网络中断后恢复下载
- 下载完成后自动清理进度文件

### 内存优化
- 流式下载，避免大文件占用过多内存
- 分片下载时按需加载数据
- 智能缓存管理

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！

### 贡献指南

1. Fork 项目
2. 创建功能分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 打开 Pull Request

### 开发规范

- 遵循 Rust 编码规范
- 添加适当的测试用例
- 更新相关文档
- 确保所有测试通过

## 更新日志

### v0.1.0
- 初始版本发布，支持下载http和https链接
- 支持多文件并发下载
- 实现断点续传功能
- 添加动态分片调整
- 统一UI进度管理
- 完整的任务管理系统 