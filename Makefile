.PHONY: all build release install clean help

TARGET := x86_64-unknown-linux-musl
BINARY := sedock

all: build

# 安装 musl target
setup:
	rustup target add $(TARGET)

# 开发构建
build:
	cargo build

# 发布构建（优化）
release:
	cargo build --release --target $(TARGET)

# 安装到系统
install: release
	sudo install -m 755 target/$(TARGET)/release/$(BINARY) /usr/local/bin/

# 清理
clean:
	cargo clean

# 测试
test:
	cargo test

# 格式化
fmt:
	cargo fmt

# Lint
lint:
	cargo clippy -- -D warnings

# 显示帮助
help:
	@echo "Available targets:"
	@echo "  setup    - Install musl target"
	@echo "  build    - Build debug version"
	@echo "  release  - Build optimized release version"
	@echo "  install  - Install to /usr/local/bin"
	@echo "  clean    - Clean build artifacts"
	@echo "  test     - Run tests"
	@echo "  fmt      - Format code"
	@echo "  lint     - Run clippy"