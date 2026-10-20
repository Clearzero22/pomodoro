# 007 · 桌面安装包完善

- **状态**: ⬜ 待开始
- **优先级**: P2
- **分支**: `feat/007-packaging`
- **依赖**: 无

## 目标

一键产出可分发的安装包。

## 需求

1. Windows：已有的 `package-windows.ps1` 补签名 + 自动检测 `ISCC`
2. macOS：`.app` 打包 + `codesign` + `dmg` (需在 Mac 上)
3. Linux：`.tar.gz` + `.deb` (需在 Linux 上)
4. CI：GitHub Actions 三平台构建

## 任务拆解

- [ ] 1. Windows 签名流程文档化
- [ ] 2. macOS 打包脚本 `scripts/package-macos.sh`
- [ ] 3. Linux 打包脚本 `scripts/package-linux.sh`
- [ ] 4. GitHub Actions workflow

## 验收

- [ ] `dist/` 下产物可直接分发安装
- [ ] 三平台 CI 绿
