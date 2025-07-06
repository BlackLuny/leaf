#!/bin/bash

# 在 macOS 平台编译 Windows 版本的 leaf-cli 脚本
# 作者: AI Assistant
# 用途: 自动化编译 Windows 版本的 leaf-cli 二进制文件

set -e  # 遇到错误立即退出

echo "🚀 开始编译 Windows 版本的 leaf-cli..."

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 检查是否在 macOS 上运行
if [[ "$OSTYPE" != "darwin"* ]]; then
    echo -e "${RED}❌ 错误: 此脚本只能在 macOS 上运行${NC}"
    exit 1
fi

# 检查是否安装了 Rust
if ! command -v rustc &> /dev/null; then
    echo -e "${RED}❌ 错误: 未找到 Rust 编译器，请先安装 Rust${NC}"
    echo "请访问 https://rustup.rs/ 安装 Rust"
    exit 1
fi

# 检查是否安装了 Homebrew
if ! command -v brew &> /dev/null; then
    echo -e "${RED}❌ 错误: 未找到 Homebrew，请先安装 Homebrew${NC}"
    echo "请访问 https://brew.sh/ 安装 Homebrew"
    exit 1
fi

echo -e "${BLUE}📦 检查并安装必要工具...${NC}"

# 检查并安装 mingw-w64
if ! brew list mingw-w64 &> /dev/null; then
    echo -e "${YELLOW}⚠️  正在安装 mingw-w64 工具链...${NC}"
    brew install mingw-w64
    echo -e "${GREEN}✅ mingw-w64 安装完成${NC}"
else
    echo -e "${GREEN}✅ mingw-w64 已安装${NC}"
fi

echo -e "${BLUE}🎯 添加 Windows 编译目标...${NC}"

# 添加 Windows 编译目标
TARGET="x86_64-pc-windows-gnu"
if ! rustup target list --installed | grep -q "$TARGET"; then
    echo -e "${YELLOW}⚠️  正在添加 $TARGET 编译目标...${NC}"
    rustup target add "$TARGET"
    echo -e "${GREEN}✅ $TARGET 编译目标添加完成${NC}"
else
    echo -e "${GREEN}✅ $TARGET 编译目标已存在${NC}"
fi

echo -e "${BLUE}🛠️  设置编译环境变量...${NC}"

# 设置编译环境变量
export CFG_COMMIT_HASH=$(git rev-parse HEAD | cut -c 1-7)
export CFG_COMMIT_DATE="$(git log --format="%ci" -n 1)"
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar

echo -e "${GREEN}✅ 环境变量设置完成${NC}"
echo -e "   - CFG_COMMIT_HASH: $CFG_COMMIT_HASH"
echo -e "   - CFG_COMMIT_DATE: $CFG_COMMIT_DATE"
echo -e "   - CC_x86_64_pc_windows_gnu: $CC_x86_64_pc_windows_gnu"
echo -e "   - AR_x86_64_pc_windows_gnu: $AR_x86_64_pc_windows_gnu"

echo -e "${BLUE}🔨 开始编译 Windows 版本...${NC}"

# 编译 Windows 版本
cargo build -p leaf-cli --release --target "$TARGET"

echo -e "${GREEN}🎉 编译成功！${NC}"

# 输出文件信息
OUTPUT_FILE="target/$TARGET/release/leaf.exe"
if [[ -f "$OUTPUT_FILE" ]]; then
    FILE_SIZE=$(du -h "$OUTPUT_FILE" | cut -f1)
    echo -e "${GREEN}📁 输出文件: $OUTPUT_FILE${NC}"
    echo -e "${GREEN}📏 文件大小: $FILE_SIZE${NC}"
    
    # 显示文件类型信息
    echo -e "${BLUE}📋 文件信息:${NC}"
    file "$OUTPUT_FILE"
    
    echo -e "${BLUE}💡 使用说明:${NC}"
    echo -e "   将 $OUTPUT_FILE 复制到 Windows 系统上即可运行"
    echo -e "   或者使用以下命令复制到当前目录:"
    echo -e "   ${YELLOW}cp $OUTPUT_FILE ./leaf-windows.exe${NC}"
else
    echo -e "${RED}❌ 编译失败: 未找到输出文件${NC}"
    exit 1
fi

echo -e "${GREEN}✨ 编译完成！${NC}" 