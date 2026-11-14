# Noctalia FreeClip 2

给 Noctalia Shell v5 用的华为 FreeClip 2 插件。顶栏、弹出面板和快捷开关是 Luau，蓝牙私有通道由一个很小的 Rust 进程 `freeclipd` 负责。

配对、音频路由和系统音量继续用 Noctalia，不在这里重做。守护进程只在 BlueZ 已经显示耳机连上之后，才打开厂商 RFCOMM 通道 1，不会调用 `Device.Connect`。

协议是按公开的 FreeClip 2 逆向结论自己写的，不内置、也不运行 OpenFreebuds。同一时间不要再开另一个 SPP 客户端。许可证是 MIT。

## 能做什么

- 顶栏电量。左键在组件旁边打开面板，右键刷新，滚轮按每格 5% 调系统音量。
- 面板第一页是电量和低延迟。均衡器、音质、语音语言、手势、摘下暂停、双设备和型号序列号各自在二级页里。取消配对是破坏性按钮。
- 控制中心里的低延迟快捷开关。
- 低电量通知。只看左右耳，同一次下降只通知一次，回升到阈值以上 5 个百分点后才能再通知。

厂商通道没连上时，面板只留状态。系统音量、静音和蓝牙仍用 Noctalia 自己的控件。

## 环境

- Noctalia v5.4.0，插件 API 43。不要用尚未随这个版本发布的 API。
- Arch Linux，BlueZ，Wayland（Niri 可以）。
- 构建需要 rustup 的 stable。这个仓库在 rustc 1.100.0 上编过。`rust-toolchain.toml` 只写了 `channel = "stable"`。
- 顶栏滚轮走 `noctalia msg volume-up` / `volume-down`。面板里不再做音量、静音或蓝牙入口。

## 构建

```sh
make build
```

二进制写到 `freeclip/bin/freeclipd`，权限 0755。这个目录在 `.gitignore` 里，不会进提交。

```sh
make check
```

`check` 会跑 `cargo test` 和 `noctalia plugins lint freeclip`。

## 安装

下面的启用命令需要 Noctalia 正在运行。本仓库没有替你执行启用。

用路径源，在仓库根目录：

```sh
noctalia msg plugins source add freeclip path "$PWD"
noctalia msg plugins enable zigzag/freeclip
```

或者复制到用户插件目录。`make install` 不会删除目录里已有的文件，也不会启用插件：

```sh
make install
noctalia msg plugins enable zigzag/freeclip
```

默认目标是 `${XDG_DATA_HOME:-~/.local/share}/noctalia/plugins/freeclip`。

然后把顶栏组件加进配置，并把 `freeclip` 放进某一条栏：

```toml
[widget.freeclip]
type = "zigzag/freeclip:battery"
```

插件 id 是 `zigzag/freeclip`。设置在 Noctalia 的插件设置里。改 `binary_path` 或 `device_address` 会重启服务；新的守护进程会按 pid 文件结束之前那个 `freeclipd`，避免两个进程抢同一条 SPP。

常用命令：

```sh
noctalia msg panel-toggle zigzag/freeclip:panel
noctalia msg plugin zigzag/freeclip:daemon all refresh
noctalia msg plugin zigzag/freeclip:daemon all latency toggle
```

## 还没在耳机上验证的部分

单元测试锁住了帧格式、CRC，以及电量、自动暂停、双击、滑动的捕获包。`cargo test` 和 `clippy -D warnings` 在 rustc 1.100 上通过了。没有耳机，所以 SPP 会话、BlueZ 发现和 Noctalia 里的实际加载都还没跑过。

FreeClip 2 的降噪读取预期会超时，超时后面板不显示该项，连接保持。佩戴状态在耳机主动上报之前是未知。当前语音语言有时要设置一次才会出现。名称匹配只认包含 `FreeClip 2` 的设备，不会误连 FreeClip 1 或 Pro。

## 许可

MIT，见 `LICENSE`。
