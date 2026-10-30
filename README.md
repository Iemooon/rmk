# lemon-nmk 分支改动

本分支基于 [rmk-rs/rmk](https://github.com/rmk-rs/rmk)，统一维护 BLE 和 NMK/Gazell 应用使用的公共补丁。当前上游基线为 [`71556128`](https://github.com/rmk-rs/rmk/commit/71556128fa72881fc35f0bf6c292d5e28ad9568d)。

## 相对上游的改动

| 改动 | 行为与启用方式 |
| --- | --- |
| USB 唤醒与重连保护 | 休眠或端点暂时不可用时保留待发报告；拔插、重枚举及切换输出后取消旧会话报告，避免补发旧按下。自动用于 USB writer。 |
| 完整按键状态接收队列 | 提供 `rmk::receiver::SnapshotQueue`，支持多来源、超时松键和溢出后恢复最新状态。应用显式接入，适用于 Gazell 等完整状态接收器。 |
| 动态层接口 | 提供 `activate_layer_if_inactive`、`deactivate_layer_if_active` 和 `is_layer_active`，用于轨迹球、鼠标等临时层控制。应用按需调用。 |
| BLE 分体断连松键 | 跟踪成功交付的按键事件；断连时仅释放对应从机的未释放按键，保留本机及其他从机输入。 |
| BLE 离线清配对 | 从机离线时保留清配对请求，持久状态更新后重新扫描发现；在线时保留原消息流程。 |
| nRF52833 DFU 窗口 | 默认关闭。启用 `nrf52833_dfu_window` 并运行窗口任务后，第 15～50 秒间断电再上电可进入 bootloader；软件 DFU 前先清除并确认持久标记。 |
| 稳定存储格式基线 | 使用 `STORAGE-BASE-COMMIT` 标识格式，避免普通实现提交导致存储身份变化；格式变化时仍须推进基线。 |
| BLE future 栈占用优化 | 原位固定并发子 future，再向 join 传递引用，减少大型临时 future 的搬运。 |
| 电量快照 | 提供 opt-in 的 WBAT v1 查询协议和接收端状态跟踪，需 `custom_message`；应用显式查询并负责左右电量与断线状态映射。 |
| 显示修饰键事件 | 暂存快速修饰键变化直到渲染器读取，不影响 HID 报告。 |

## 接入与兼容性

- BLE/Gazell 协议选择、无线参数、引脚、矩阵、USB 标识、Vial 解锁和 Flash 分区由消费工程配置；本分支不包含 Gazell 无线实现。
- DFU 窗口仅用于 nRF52833，必须注册并运行任务；接入前移除旧 common 窗口任务，避免重复运行。
- 旧版存储格式、feature 或容量配置变化可能清除改键、宏及配对。本次上游同步保留存储格式基线，实际工程仍需验证配对与重连。

## 分支文档

- [完整特性、源码入口与验证记录](LEMON-NMK-FEATURES.md)
- [nRF52833 DFU 接入](LEMON-NMK-DFU.md)
- [完整状态接收队列接入](LEMON-NMK-RECEIVER.md)
- [补丁归属与上游同步记录](LOCAL-PATCHES.md)
