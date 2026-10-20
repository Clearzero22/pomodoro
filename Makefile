# 番茄钟：桌面端与浏览器端共用的构建入口。
#
#   make desktop          # 开发模式运行桌面应用
#   make desktop-release  # 构建发布版桌面可执行文件
#   make wasm             # 编译 WASM（debug）
#   make wasm-release     # 编译 WASM（release）
#   make web              # 启动浏览器开发服务器
#   make web-release      # 生成 www/dist 生产包
#   make mobile-sync      # web 产物 + 同步进 Capacitor 壳
#   make mobile-assets    # 生成全平台图标与启动图
#   make android          # 构建 Android debug APK
#   make test             # 跑行为测试
#   make all              # 桌面发布版 + 浏览器生产包

SHELL := /bin/bash
WEB := crates/pomodoro-web
WWW := $(WEB)/www
CAP := ./node_modules/.bin/capacitor

# Gradle 8.11.1 不支持 JDK 25（报 Unsupported class file major version 69），
# 所以默认用项目内的 JDK 21；换成自己的 JDK 17-23 用 `make android ANDROID_JDK=/path`。
ANDROID_JDK ?= $(CURDIR)/tools/jdk-21.0.12.1+1

.PHONY: help desktop desktop-release wasm wasm-release web web-release test all \
        mobile-sync mobile-assets android clean

help:
	@grep -E '^[a-z-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

desktop: ## 开发模式运行桌面应用
	cargo run -p pomodoro-desktop

desktop-release: ## 构建发布版桌面可执行文件
	cargo build --release -p pomodoro-desktop

wasm: ## 编译 WASM（debug）
	cd $(WEB) && ./scripts/build-wasm.sh

wasm-release: ## 编译 WASM（release）
	cd $(WEB) && ./scripts/build-wasm.sh --release

web: wasm ## 启动浏览器开发服务器
	@test -d $(WWW)/node_modules || (cd $(WWW) && bun install)
	cd $(WWW) && bun run dev

web-release: wasm-release ## 生成生产包到 www/dist
	@test -d $(WWW)/node_modules || (cd $(WWW) && bun install)
	cd $(WWW) && bun run build

test: ## 跑行为测试
	cargo test -p pomodoro

all: desktop-release web-release ## 两端发布构建

mobile-sync: web-release ## 把最新 web 产物同步进移动端壳
	cd mobile && $(CAP) sync

mobile-assets: ## 从 mobile/assets 生成全平台图标与启动图
	cd mobile && ./node_modules/.bin/capacitor-assets generate --android

android: web-release ## 构建 Android debug APK（需 Android SDK + JDK 17-23）
	cd mobile && $(CAP) sync
	cd mobile/android && JAVA_HOME="$(ANDROID_JDK)" ./gradlew.bat assembleDebug
	@mkdir -p dist
	cp mobile/android/app/build/outputs/apk/debug/app-debug.apk dist/pomodoro-android-debug.apk
	@echo "==> dist/pomodoro-android-debug.apk"

clean:
	cargo clean
	rm -rf $(WWW)/dist $(WWW)/src/wasm
