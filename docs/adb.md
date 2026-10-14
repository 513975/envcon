# ADB / Android Platform Tools 管理

EnvCon 0.5.5 起将 ADB 作为 Android SDK Platform Tools 环境管理。它与完整 Android SDK 分开：纳入已有 SDK 时只链接 `platform-tools` 目录，不移动或删除 SDK、模拟器、构建工具和系统镜像。

## 发现与纳管

系统扫描会从用户和系统 PATH、`ANDROID_HOME`、`ANDROID_SDK_ROOT` 以及 `%LOCALAPPDATA%\Android\Sdk\platform-tools` 查找 `adb.exe`。只有同时包含 `adb.exe`、`AdbWinApi.dll` 和 `AdbWinUsbApi.dll` 的目录才可纳入管理。

纳入管理会在 `envs\adbs` 创建指向原 `platform-tools` 的目录链接。移除该外部环境只删除链接；原 SDK 目录保持不变。EnvCon 自己下载的版本是普通受管目录，卸载会删除该受管版本。

## 版本与下载

版本优先读取官方 ZIP 内的 `source.properties`。命令输出中的 `Version 37.0.1-...` 可作为发行版本回退；`Android Debug Bridge version 1.0.41` 是 ADB 协议版本，只在无法获得发行版本时明确标注，避免误显示为 Platform Tools 版本。

下载中心读取 Google 官方 Android SDK 仓库索引，只显示当前 Windows 稳定版。官方索引通常不保留完整历史版本。下载链接必须与所选版本精确对应，解压后还会检查 ADB、Fastboot、运行库和版本元数据。

## 切换与占用

切换会更新 `current\adb`，将该目录加入用户 PATH 后，新终端会使用当前版本。已经运行的 ADB server 不会因切换目录自动重启；版本未生效或卸载提示文件占用时，先执行 `adb kill-server`，再重新打开终端或重试卸载。

使用 Android SDK Platform Tools 需遵守 Google Android SDK 许可条款。0.5.5 EXE 与 NSIS 安装包已在本地构建，版本及哈希见 [构建记录](release-0.5.5.md)；未正式发布 GitHub Release。
