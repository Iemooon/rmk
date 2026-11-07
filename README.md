# lemon-nmk 分支改动

本分支基于 [rmk-rs/rmk](https://github.com/rmk-rs/rmk)，统一维护 BLE 和 NMK/Gazell 应用使用的公共补丁。当前上游基线为 [`bba0f24bf`](https://github.com/rmk-rs/rmk/commit/bba0f24bf59e142f684c38c37193ae5a45653e9d)。

## 相对上游的改动

路径均相对于本仓库根目录；“显式接入”表示仅更新 RMK 依赖不会自动开启。

| 改动 | 行为与启用方式 | 源码位置 |
| --- | --- | --- |
| A/B 接收器切换 | 应用显式接入，管理切换、松键确认和唯一接收器接管。当前切换协议固定支持两个 A/B 接收器；每个接收器跟踪左右两个来源，每个来源传四行数据。 | [receiver/switching.rs](rmk/src/receiver/switching.rs) |
| 切换时清理按键状态 | 应用显式调用切换运行入口或清理接口，清除 LT 长按、临时层、修饰键、宏和矩阵按住状态，并排队发送全松报告。常规运行入口不触发。 | [keyboard.rs](rmk/src/keyboard.rs)、[keymap.rs](rmk/src/keymap.rs)、[keyboard/macros.rs](rmk/src/keyboard/macros.rs)、[matrix.rs](rmk/src/matrix.rs) |
| 独立电池电压记录 | 应用显式使用 `BatteryStore<N>`，保存各来源的电池类型、最终 mV 和收包时间；不负责采样或百分比换算。 | [receiver/battery.rs](rmk/src/receiver/battery.rs) |
| USB 电压查询接口 | 应用显式注册并运行独立 HID，通过 EBAT/1 向上位机提供左右电池信息。 | [battery_protocol.rs](rmk/src/receiver/battery_protocol.rs)、[battery_usb.rs](rmk/src/receiver/battery_usb.rs) |
| Gazell 无线库集中分发 | 显式添加 `rmk-gazell` 构建依赖，提供原始 SDK 17.1.1 库（现有 52840/52833 接收器）和 nrfxlib 2.9.0 库（52820 soft-float），保留许可证。 | [构建辅助代码](rmk-gazell/src/lib.rs)、[库文件与许可证目录](rmk-gazell/vendor/gzll/) |
| USB 唤醒与重连保护 | 休眠或端点暂时不可用时保留待发报告；拔插、重枚举及切换输出后取消旧会话报告，避免补发旧按下。自动用于 USB writer。 | [usb/mod.rs](rmk/src/usb/mod.rs)、[state.rs](rmk/src/state.rs) |
| 完整按键状态接收队列 | 提供 `rmk::receiver::SnapshotQueue`，支持多来源、超时松键和溢出后恢复最新状态。应用显式接入，适用于 Gazell 等完整状态接收器。 | [receiver.rs](rmk/src/receiver.rs) |
| 动态层接口 | 提供 `activate_layer_if_inactive`、`deactivate_layer_if_active` 和 `is_layer_active`，用于轨迹球、鼠标等临时层控制。应用按需调用。 | [keymap.rs](rmk/src/keymap.rs) |
| BLE 分体断连松键 | BLE 分体路径自动跟踪成功交付的按键事件；断连时仅释放对应从机的未释放按键，保留本机及其他从机输入。 | [split/pressed.rs](rmk/src/split/pressed.rs)、[split/driver.rs](rmk/src/split/driver.rs) |
| BLE 离线清配对 | BLE 中央端的清配对操作触发。从机离线时保留请求，持久状态更新后重新扫描发现；在线时保留原消息流程。 | [split/ble/central.rs](rmk/src/split/ble/central.rs)、[keyboard.rs](rmk/src/keyboard.rs) |
| nRF52833 DFU 窗口 | 默认关闭。启用 `nrf52833_dfu_window` 并运行窗口任务后，第 15～50 秒间断电再上电可进入 bootloader；软件 DFU 前先清除并确认持久标记。 | [dfu_window.rs](rmk/src/dfu_window.rs)、[dfu_window/policy.rs](rmk/src/dfu_window/policy.rs)、[boot.rs](rmk/src/boot.rs) |
| 稳定存储格式基线 | 存储构建自动使用 `STORAGE-BASE-COMMIT` 标识格式，避免普通实现提交导致存储身份变化；格式变化时仍须推进基线。 | [STORAGE-BASE-COMMIT](STORAGE-BASE-COMMIT)、[rmk/build.rs](rmk/build.rs) |
| BLE future 栈占用优化 | 自动用于 BLE 中央端对应任务组合，原位固定并发子 future，再向 join 传递引用，减少大型临时 future 的搬运。 | [ble/mod.rs](rmk/src/ble/mod.rs) |
| Vial 休眠时间设置 | 启用 `vial` 和 BLE 芯片 feature 后，可通过自定义通道读写空闲休眠秒数；`0` 关闭空闲超时休眠，`30～3600` 有效。启用 `storage` 时持久保存，无需 Rynk。 | [host/via/mod.rs](rmk/src/host/via/mod.rs)、[ble/sleep.rs](rmk/src/ble/sleep.rs)、[storage/mod.rs](rmk/src/storage/mod.rs) |
| 显示接收器电量快照 | 提供按需 WBAT v1 查询和接收端状态跟踪，需 `custom_message`；应用显式接入并负责左右电量与断线状态映射。与 EBAT 电压接口独立。 | [battery_snapshot.rs](rmk/src/battery_snapshot.rs)、[battery_snapshot_responder.rs](rmk/src/battery_snapshot_responder.rs) |
| 显示修饰键事件 | 显示处理器暂存快速修饰键变化直到渲染器读取，不影响 HID 报告；应用渲染器可读取暂存状态。 | [display/mod.rs](rmk/src/display/mod.rs) |

### 切换清理默认不会影响普通键盘

普通键盘继续使用 `Keyboard` 的 `Runnable::run()`，不会自动执行切换清理，也没有新增的 5 ms 切换轮询。
需要此功能的接收器主动使用 `run_with_handover()`，只有 `needs_reset` 回调返回 true 才清理；
应用也可以直接调用 `reset_for_handover()`。这里通过显式 API 调用启用，没有单独的 Cargo feature 开关。

`external_user_actions` 默认是 false，普通 BLE User 键行为保持原有路径。
`run_with_handover()` 会开启外部 User 动作模式，由应用的 ActionEvent 订阅者处理 User 键。
清理回调完成仅表示状态已清理、全松报告已排队；切换应用仍须确认 USB 报告实际写入端点后再允许 COMMIT。

### Vial 休眠时间设置

启用 `vial` 和 BLE 芯片 feature（例如 `nrf52840_ble`）后，固件提供一个 RMK 自定义 Vial 通道，供 `wk-kbd-app` 等上位机配置。`storage` 用于持久保存；未启用时设置仅在本次运行有效。

请求和回复均为 32 字节报告，未使用字节填零：

- 读取：字节 0 为 `0x08`（`CustomGetValue`），字节 1 为通道 `0xF0`；回复字节 2～5 为大端 `u32` 秒数。
- 设置：字节 0 为 `0x07`（`CustomSetValue`），字节 1 为 `0xF0`，字节 2～5 为大端 `u32` 秒数。成功时回复保留命令及设置值，不需要额外发送 `CustomSave`。
- `0` 关闭空闲超时休眠，`30～3600` 秒有效；无效值或存储写入失败时，回复字节 0 为 `0xFF`。写入失败会恢复原设置。
- 未启用 BLE 的固件收到通道 `0xF0` 的读取或设置请求时，回复字节 0 为 `0xFF`，表示不支持。

没有有效持久设置时，初值来自 `[rmk].split_central_sleep_timeout_seconds`，默认 `0`。名称虽含 split，管理器也用于非分体 BLE 键盘。上述范围约束针对运行时设置接口。

清醒时修改设置会通知管理器按新值重新计时；已经休眠时不会立即唤醒，仍需按键或指针等活动，随后应用新值。`0` 不阻止主机 HID 挂起、广播超时等主动休眠请求。这里控制的是 RMK 逻辑休眠，不代表关闭蓝牙或进入芯片 SYSTEMOFF，也不保证断线重连后保留用于唤醒的第一次输入。

启用 `storage` 时，设置写入成功后可在重启时恢复。新增存储项追加在枚举末尾，既有记录编号不变，保留 `STORAGE-BASE-COMMIT`；只有版本、feature 和容量等存储身份一致时才能沿用旧记录。回退旧固件无法识别新增项，未验证回退兼容性。

这是 Vial Raw HID 扩展，不需要启用 Rynk，也不会改变标准 Vial 键位、宏和灯光命令。固件重新编译后上位机才可使用；静态编译检查不代表实机功耗或休眠唤醒已验收。

## 接入与兼容性

- BLE/Gazell 协议选择、无线参数、引脚、矩阵、USB 标识、Vial 解锁和 Flash 分区由消费工程配置。`rmk-gazell` 只提供原始无线库与链接入口；应用仍负责 FFI、IRQ/glue 和无线初始化。
- DFU 窗口仅用于 nRF52833，必须注册并运行任务；接入前移除旧 common 窗口任务，避免重复运行。
- 旧版存储格式、feature 或容量配置变化可能清除改键、宏及配对。本次上游同步保留存储格式基线，实际工程仍需验证配对与重连。

## 分支文档

- [完整特性、源码入口与验证记录](LEMON-NMK-FEATURES.md)
- [nRF52833 DFU 接入](LEMON-NMK-DFU.md)
- [接收队列、A/B 切换和电压接口接入](LEMON-NMK-RECEIVER.md)
- [Gazell 构建依赖与许可证](rmk-gazell/README.md)
- [补丁归属与上游同步记录](LOCAL-PATCHES.md)
