# lemon-nmk 新增特性与维护记录

最后更新：2026-10-27。本文是 `Iemooon/rmk` 的 `lemon-nmk` 分支特性总表。
后续新增、修改、移除本地特性及同步上游时，应在同一提交中更新本文。

## 分支用途与基线

本分支统一承载 BLE 和 NMK/Gazell 应用需要的 RMK 公共能力。
分支名称不会选择无线协议；实际协议、RF 参数、引脚、矩阵、USB 名称、分区、
Vial 解锁设置仍由各固件工程配置。Gazell 无线收发实现也仍属于应用工程。

- 当前上游基线：`5ed4fd6fc9059107612004a3416b7d2569fbb045`。
- 原 `dynamic-layers` 已完整合入本分支，旧分支已删除。
- 原 `vendor/rmk`、`vendor/rmk-cck`、`vendor/rmk-lily58` 的公共补丁已核对覆盖。
- 库功能统一不代表所有键盘已迁移依赖，也不代表所有实物已验收。

## 特性总表

| 特性 | 适用范围 | 如何开启 | 源码入口 |
| --- | --- | --- | --- |
| USB 唤醒及重连报告保护 | 使用 USB HID 的 BLE/Gazell 应用 | USB writer 自动使用 | [usb/mod.rs](rmk/src/usb/mod.rs)、[state.rs](rmk/src/state.rs) |
| 完整状态接收队列与超时松键 | Gazell 等完整按键状态接收器 | 应用显式使用 `SnapshotQueue` | [receiver.rs](rmk/src/receiver.rs)、[接入说明](LEMON-NMK-RECEIVER.md) |
| 动态层控制接口 | 轨迹球、鼠标等输入处理器，与 BLE/Gazell 无关 | 应用调用接口，无单独 feature | [keymap.rs](rmk/src/keymap.rs) |
| BLE 分体断连松键 | BLE 分体连接 | 启用 BLE 芯片及 `split`，对应路径自动使用 | [driver.rs](rmk/src/split/driver.rs)、[pressed.rs](rmk/src/split/pressed.rs) |
| 离线清配对后重新发现 | BLE 分体中央端 | 原有清配对操作触发 | [central.rs](rmk/src/split/ble/central.rs)、[keyboard.rs](rmk/src/keyboard.rs) |
| 15～50 秒断电进入 DFU | **仅 nRF52833** | `nrf52833_dfu_window` 加后台任务，默认关闭 | [dfu_window.rs](rmk/src/dfu_window.rs)、[policy.rs](rmk/src/dfu_window/policy.rs) |
| 软件进入 DFU 前清持久标记 | **52833 窗口任务的配套行为** | 与上项一起启用，不需要修改 Vial 协议 | [boot.rs](rmk/src/boot.rs)、[接入说明](LEMON-NMK-DFU.md) |
| 稳定存储格式基线 | 启用存储的应用 | 自动使用分支中的基线文件 | [STORAGE-BASE-COMMIT](STORAGE-BASE-COMMIT)、[build.rs](rmk/build.rs) |
| BLE 并发任务减少临时栈占用 | BLE 键盘中央端 | 自动使用，不改变协议与任务调度顺序 | [ble/mod.rs](rmk/src/ble/mod.rs) |

### USB 唤醒与重连保护（2026-10-27）

从 `dongle-tft-st7789` 的 `vendor/rmk-ble`、`vendor/rmk-gazell` 公共补丁迁入。
USB 休眠时保留已取出的报告，按顺序等待恢复；端点暂时 Disabled 时保留同一报告重试。
拔插、reset/unconfigure 或离开 USB 输出时推进会话编号，取消旧会话尚未完成的报告，
避免重新连接后补发旧按下。正常 suspend/resume 不取消报告。
默认用于 USB 报告发送路径，BLE/Gazell 应用均可使用，不改变无线帧或存储格式。
队列仍有容量限制和背压；主机不允许远程唤醒时不会强行唤醒，其他发送错误仍记录后丢弃。

### Gazell 通用完整状态队列（2026-10-27）

新增 `rmk::receiver::SnapshotQueue<BYTES, SOURCES, CAP>`，从 TFT 工程的
Gazell 状态机抽取，并把固定两端扩展为任意编译期发射端数量。无需新增 Cargo feature，
只有应用实例化并接入后才生效，不自动替换任何键盘的接收流程。

- 使用单调毫秒接收时间和应用指定的超时；重复合法状态刷新存活，非法长度不刷新。
- 超时丢弃该来源的积压状态，产生一次全零松键状态，其他来源继续保序。
- 正常容量内保留连续按下/松开；溢出时按各来源最新状态恢复，无法补回已丢失的短按。
- 重连且未溢出时保留先松开再按下的边界；溢出恢复优先保证最终状态。
- `seen_within` 的接收历史独立于松键超时，可供连接指示使用。

仅适用于固定长度的完整按键状态，且全零表示松开；不适用于增量事件、相对鼠标位移或
把电池/序号等 RF 头部直接放进状态。应用负责验证帧、排空 RF FIFO、同步访问、消抖、
矩阵映射，以及完整发布一个快照的所有变化后再处理下一个。详见
[接入说明](LEMON-NMK-RECEIVER.md)。这不改变既有 Gazell 地址、频道、协议或接线。

### 动态层

提供 `activate_layer_if_inactive`、`deactivate_layer_if_active`、`is_layer_active`。
开启接口返回是否实际开启，调用者可据此记录自己是否需要在退出时关闭该层。
接口本身不提供多调用者引用计数，应用需协调实体层键和鼠标层的使用关系。
普通键盘无需调用，不影响 Gazell 协议。

### BLE 分体可靠性

只记录成功交付的按键事件；断开某个从机时，仅释放该从机尚未释放的按键。
释放使用全局矩阵坐标，不清除本机或另一半的输入。
处理事件发送被取消、队列背压、重复事件、无效坐标及跟踪容量不足等情况。

从机不在线时，清配对请求仍可保留到扫描/连接流程，确认存储更新后重新发现。
在线从机沿用原来的清配对消息路径。此修复针对 BLE 分体，不修改 Gazell 帧格式。

### 52833 DFU 窗口与软件跳转

第 15 秒写 ARMED，第 50 秒清除。持续供电不会自动跳转；窗口内断电再上电后，
清除并读回确认标记，再进入 bootloader。带电池时仅拔 USB 不算断电。

软件 DFU 请求先停止窗口写标记，再清除并读回确认，失败时留在应用。
默认关闭，52820/52840 不开启。底层准备接口保留兼容性，其他应用不会因启用
`storage` 自动拦截跳转。具体开启方式、任务注册、槽位和限制见
[52833 DFU 接入说明](LEMON-NMK-DFU.md)。不要同时运行旧 common 任务和分支内任务。

### 存储兼容性

### BLE future 栈占用修复

CCK 迁移构建发现 BLE `join4` 的临时栈帧为 17792 字节，超过现有 16 KiB 检查阈值。
改为原位 pin 四个子 future，再将引用传入 join，避免完成状态转换时搬运大 future。
不调整原有栈检查阈值。修复后 CCK MO 探针构建最大单帧为 14272 字节，检查通过；
主机 Vial/BLE 分体回归 636 项通过。完整固件及实物验收由消费工程另行记录。

### 存储兼容性说明

普通实现提交不再直接作为存储格式身份，使用 `STORAGE-BASE-COMMIT`。
格式变更时必须更新该基线，不能把旧身份强加到不兼容的新格式上。
校验仍包含 RMK 版本、feature 列表及相关容量配置，修改这些内容也可能清除旧设置。

当前基线已包含上游 `MacroData` 到 `MacroChunk` 及存储动作类型升级。
旧版升级可能清除改键、宏和配对；不要宣称无损迁移。

## 验证记录

记录必须注明代码版本，不能把旧版测试结果当作后续提交重新测试的结果。

| 验证对象 | 代码版本 | 结果 | 验证范围 |
| --- | --- | --- | --- |
| Vial/BLE 分体主机测试 | `bd07c8535` | 628 项通过 | 原公共补丁与上游同步后的回归 |
| Rynk/BLE 分体主机测试 | `bd07c8535` | 675 项通过 | Rynk 与 BLE 路径 |
| 不启用 BLE 的分体主机测试 | `bd07c8535` | 574 项通过 | 无 BLE 配置的回归 |
| Vial/BLE 分体主机测试 | `6107d5861` | 636 项通过 | 包含迁入窗口、清标记失败/重试、软件请求取消读取测试 |
| 52833 开启窗口，ARM 库编译 | `6107d5861` | debug/release 通过 | 库的目标编译，不含完整固件链接及分区检查 |
| 52840 关闭窗口，ARM 库编译 | `6107d5861` | release 通过 | 其他芯片不启用的构建 |
| 窗口与 52840 HAL 同时开启 | `6107d5861` | 按预期拒绝编译 | 错误芯片组合检查 |

主机测试通用命令，在仓库根目录执行：

```powershell
cargo nextest run --manifest-path rmk/Cargo.toml --config-file .config/nextest.toml --no-default-features --features split,vial,storage,async_matrix,_ble
```

测试通过不等于硬件验收。每个实际工程仍须完整编译、检查 Flash/RAM，
并验证输入、配对、重连、窗口与 Vial 进入 DFU、重复刷写及 USB 重新枚举。
旧 common 实现的现场反馈不作为分支内新模块的实物验收记录。

### 本次接收器可靠性补丁验证（2026-10-27）

代码范围：`e8e1e714e` 之后本次提交的 USB 与完整状态队列补丁。

| 检查 | 结果 | 范围 |
| --- | --- | --- |
| Vial/BLE：`split,vial,storage,async_matrix,_ble` | 648 项通过 | 主机回归，含 10 项队列测试及会话测试 |
| Vial 无 BLE：`split,vial,storage,async_matrix` | 594 项通过 | 主机回归 |
| Rynk/BLE：`rynk,_ble,split,async_matrix,storage` | 695 项通过 | 主机回归 |
| `python scripts/check_usb_wakeup.py` | 7 项通过 | 提取实际生产 writer，模拟端点和时钟 |
| 旧 writer 负对照 | 同一工具 4 项失败、3 项通过 | 确认能够检出旧行为，不是编译失败 |
| 去掉队列超时/溢出恢复的负对照 | 分别 5 项、2 项失败 | 验证测试能检出保护缺失 |
| 52840 BLE，`thumbv7em-none-eabihf` release 库检查 | 通过 | `vial,storage,nrf52840_ble,embassy-nrf/nrf52840` |
| 52833 无 BLE，同目标 release 库检查 | 通过 | `vial,storage,embassy-nrf/nrf52833` |

ARM 命令使用 `cargo check --manifest-path rmk/Cargo.toml --target thumbv7em-none-eabihf
--release --no-default-features --features <features>`。库检查不包括最终固件链接和分区验收。
USB 工具已接入 `scripts/test_all.sh` 和 `.github/ci/test.sh`；未运行完整 CI 脚本。
本次未迁移消费工程依赖、未构建最终键盘固件、未烧录或做实机验证。
不修改存储格式、不推进基线。会话编号使用 RMK 原有互斥机制，不要求额外原子指令能力。

## 后续维护规则

每次修改本分支时，在同一提交或同一 PR 中完成：

1. 更新本文的特性、适用芯片、默认状态、接口及源码入口；删除特性也要记录。
2. 对 DFU 行为修改同步更新 `LEMON-NMK-DFU.md`；对上游基线或补丁归属修改同步更新 `LOCAL-PATCHES.md`。
3. 检查持久格式和 feature 是否影响旧设置，写清迁移结果；必要时推进存储基线。
4. 运行与改动相关的测试，记录代码版本、命令、结果；未做的硬件验收写明待验证。
5. 同步上游时逐项核对此表，避免重复补丁或被新 API 替换后遗漏行为。
6. 记录消费工程的实际迁移和验收证据；没有检查时不列为“已兼容”或“已迁移”。

日常维护随代码变更执行，不依靠定时任务。本文只维护 fork 新增/保留的差异，
上游通用功能继续查阅 RMK 原文档。

## 变更记录

| 日期 | 提交或事件 | 变更 |
| --- | --- | --- |
| 2026-10-27 | 接收器可靠性补丁 | 迁入 USB 唤醒/重连保护，新增可复用完整状态队列和回归；无存储格式变化 |
| 2026-10-27 | `c5586b1e5`、`bd07c8535` | 合入动态层和上游更新，推进存储格式基线 |
| 2026-10-27 | `e3623fee5` | 明确 BLE/NMK 统一分支及公共补丁范围 |
| 2026-10-27 | `6107d5861` | 新增 52833 可选 DFU 窗口及配套清标记任务 |
| 2026-10-27 | 文档体系建立 | 建立本文、首页入口及随代码更新的维护规则；删除已合入的 dynamic-layers 分支 |
| 2026-10-27 | BLE future 栈占用修复 | 针对 CCK 迁移构建，join 原位固定的子 future 引用，保留原栈检查阈值；无存储格式变化 |
