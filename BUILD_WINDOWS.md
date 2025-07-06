# Windows 编译脚本使用说明

## 概述

`build-windows.sh` 是一个用于在 macOS 平台编译 Windows 版本 `leaf-cli` 的自动化脚本。

## 系统要求

- macOS 系统（M1/M2 或 Intel 芯片）
- Rust 编译器（通过 [rustup](https://rustup.rs/) 安装）
- Homebrew 包管理器（通过 [brew.sh](https://brew.sh/) 安装）

## 使用方法

### 1. 赋予脚本执行权限

```bash
chmod +x build-windows.sh
```

### 2. 运行脚本

```bash
./build-windows.sh
```

脚本会自动完成以下任务：

1. ✅ 检查系统环境（macOS、Rust、Homebrew）
2. 📦 安装必要工具（mingw-w64 交叉编译工具链）
3. 🎯 添加 Windows 编译目标（x86_64-pc-windows-gnu）
4. 🛠️ 设置编译环境变量（包括 Git 提交信息）
5. 🔨 编译 Windows 版本的 leaf-cli
6. 📋 显示编译结果和文件信息

### 3. 编译完成后

编译成功后，Windows 版本的可执行文件将位于：
```
target/x86_64-pc-windows-gnu/release/leaf.exe
```

您可以将此文件复制到 Windows 系统上运行，或者使用以下命令将其复制到当前目录：

```bash
cp target/x86_64-pc-windows-gnu/release/leaf.exe ./leaf-windows.exe
```

## 输出示例

脚本运行时会显示彩色的进度信息：

```
🚀 开始编译 Windows 版本的 leaf-cli...
📦 检查并安装必要工具...
✅ mingw-w64 已安装
🎯 添加 Windows 编译目标...
✅ x86_64-pc-windows-gnu 编译目标已存在
🛠️  设置编译环境变量...
✅ 环境变量设置完成
🔨 开始编译 Windows 版本...
🎉 编译成功！
📁 输出文件: target/x86_64-pc-windows-gnu/release/leaf.exe
📏 文件大小: 13M
📋 文件信息:
target/x86_64-pc-windows-gnu/release/leaf.exe: PE32+ executable (console) x86-64 (stripped to external PDB), for MS Windows
💡 使用说明:
   将 target/x86_64-pc-windows-gnu/release/leaf.exe 复制到 Windows 系统上即可运行
✨ 编译完成！
```

## 故障排除

### 1. 脚本无法运行

确保脚本有执行权限：
```bash
chmod +x build-windows.sh
```

### 2. 缺少 Rust 编译器

安装 Rust：
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### 3. 缺少 Homebrew

安装 Homebrew：
```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

### 4. 编译失败

如果遇到编译错误，请检查：
- 网络连接是否正常（需要下载依赖）
- 磁盘空间是否足够（编译需要一定空间）
- 是否有其他 Rust 项目正在编译

## 技术细节

- **编译目标**: `x86_64-pc-windows-gnu`
- **工具链**: mingw-w64
- **输出格式**: PE32+ Windows 可执行文件
- **架构**: 64位 x86_64
- **类型**: 控制台应用程序

## 注意事项

1. 首次运行时，脚本会自动安装 mingw-w64 工具链，这可能需要几分钟时间
2. 编译过程中会下载 Rust 依赖包，需要网络连接
3. 生成的可执行文件约为 13MB，已经过优化（release 模式）
4. 编译的二进制文件可以在 Windows 7 及以上版本的 64位系统上运行

## 手动编译（如果脚本不可用）

如果脚本无法使用，您也可以手动执行以下命令：

```bash
# 安装 mingw-w64
brew install mingw-w64

# 添加 Windows 编译目标
rustup target add x86_64-pc-windows-gnu

# 设置环境变量并编译
export CFG_COMMIT_HASH=$(git rev-parse HEAD | cut -c 1-7)
export CFG_COMMIT_DATE="$(git log --format="%ci" -n 1)"
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar

# 编译
cargo build -p leaf-cli --release --target x86_64-pc-windows-gnu
``` 