# MultiDown

一个用 Rust 编写的多线程下载管理器，支持并发下载、断点续传和进度显示。

## 功能特点

- 多文件并发下载
- 实时进度显示
- 断点续传
- 自动重试机制
- 可配置的下载参数
- 支持 HTTP/HTTPS/FTP 协议
- 详细的日志记录

## 后续计划

- 支持更多的下载协议
- 提供更友好的用户界面
- 增加资源搜索和管理功能
- 增加更多的下载选项和配置
- 优化性能和稳定性

## 安装

确保你已经安装了 Rust 和 Cargo。然后运行：

```bash
git clone https://github.com/panzhifu/Multidown.git
cd multidown
cargo build --release
```

## 使用方法

基本用法：

```bash
cargo run -- https://example.com/file1.zip https://example.com/file2.zip
```

或者使用编译后的二进制文件：

```bash
./target/release/multidown https://example.com/file1.zip https://example.com/file2.zip
```

### 快捷键

- `q` 或 `Esc`: 暂停下载并退出
- `Ctrl+C`: 暂停下载并退出

## 配置

配置文件支持以下选项：

- 并发下载数
- 默认线程数
- 下载速度限制
- 输出目录
- 重试次数和延迟
- 超时时间
- 用户代理
- 代理设置
- SSL验证
- 文件处理选项

## 开发

```bash
# 运行测试
cargo test

# 检查代码
cargo check

# 格式化代码
cargo fmt
```

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！ 