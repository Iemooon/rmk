# 通用接收状态与 USB 可靠性

`rmk::receiver` 来自 `dongle-tft-st7789/keyboards/ergo52/gazell_state.rs`
的硬件无关队列；已扩展为可配置来源数量，未移入键盘专用矩阵转换。

## 接入完整按键状态队列

```rust
use rmk::receiver::SnapshotQueue;

// 每端解码后 4 字节，2 个发射端，最多排队 32 份状态。
let mut queue = SnapshotQueue::<4, 2, 32>::new();
let timeout_ms = 500; // 示例；按实际心跳与休眠策略配置
let received_at_ms = 10;
queue.receive(0, &[1, 0, 0, 0], received_at_ms, timeout_ms);
while let Some(snapshot) = queue.next_report(received_at_ms, timeout_ms) {
    // 与应用保留的该来源已发布状态比较，逐个发布矩阵变化。
    // 完整处理 snapshot 后再取下一份；不要在发送途中替换它。
    let _ = (snapshot.source, snapshot.state);
}
```

生产应用应在无包时也定期调用 `next_report`，使用真正的当前单调毫秒时间，
否则无法及时松键。接收端休眠/发射端心跳间隔必须与超时匹配。
快照每位的矩阵含义、全局坐标和发布背压由应用决定。队列不自动发布 RMK 事件，
也不连接 Gazell 库；中断和任务间访问必须由应用同步。

RF 帧必须先检查有效性并解码。仅成功检查的完整状态才能调用 `receive`。
重复完整状态是心跳；长度错误或来源编号越界返回 false，后者不计入某来源统计。
容量至少为 `max(SOURCES, 2)`，状态字节数和来源数必须大于零。
全零必须代表所有键松开。相对位移、增量事件、滚轮、电量等不经过本队列。

溢出只保证恢复各来源的最终状态，不保证还原丢失短按或重连边沿。
这份库代码不会自动让现有固件获得保护，消费工程须显式迁移并验证无线、
断连松键、连续按键、队列溢出与矩阵映射。

## USB 自动保护

`UsbKeyboardWriter` 保留唤醒首份报告，暂停消费后续报告，恢复后继续发送。
端点 Disabled 时持续保留当前报告重试；不改变原有限容量和背压。
会话编号在 reset/unconfigure、拔插、离开 USB 输出时失效，旧 pending 写入会取消。
挂起恢复保持编号。未启用主机远程唤醒或主机持续离线时，不承诺输入能够无限缓存。
非 Disabled 的发送错误仍记录并放弃当前报告。

## 回归命令

在仓库根目录运行：

```powershell
python scripts/check_usb_wakeup.py
cargo nextest run --manifest-path rmk/Cargo.toml --config-file .config/nextest.toml --no-default-features --features split,vial,storage,async_matrix,_ble
```

Python 工具提取实际生产 writer 和会话判定函数，放入受控端点/时钟环境测试。
它覆盖时序行为，不是真实 USB 控制器或 Gazell 空口测试。
队列单元测试随 RMK 主机测试运行，不需要 Gazell 硬件。
本次不改变存储格式，不推进 `STORAGE-BASE-COMMIT`。

## A/B 切换与电压接口（2026-11-01）

以下功能只有应用显式调用才启用，不改变普通键盘或 BLE User 键行为：

| 模块/API | 职责 | 应用负责 |
| --- | --- | --- |
| `receiver::switching::Control` | 两接收器、两半、四行的 Gazell v4 / ACK v2 状态机 | 无线收发、单调时钟、USB 完成确认 |
| `switching::Config` | User ID 和断连超时；默认 `[0,1,2]`、700 ms | 选择 A/B 身份与板级参数 |
| `Keyboard::run_with_handover` | 事件过滤、清理 LT/宏/修饰键/临时层/矩阵状态，发送全松报告 | 提供 allow、needs_reset、reset_done 回调 |
| `Keyboard::set_external_user_actions` | 把 User 动作交给应用 ActionEvent 订阅者 | 注册订阅者，避免同时使用 BLE User 动作 |
| `receiver::battery::BatteryStore<N>` | 保存各来源的类型、最终 mV、最近收包时间 | 解码无线包、同步 ISR/任务访问 |
| `receiver::battery_protocol::reply` | 双侧 EBAT/1 32 字节 USB 应答 | 提供快照与接收器身份/工作状态 |
| `receiver::battery_usb::{build,run}` | 独立 FF62/52 HID，100 ms 写入超时 | USB builder 中注册一次，并运行任务 |

`run_with_handover` 代替普通 Keyboard run，不能同时运行；会自动开启 external User 模式。
`reset_done` 仅表示内存清理和排队完成，应用仍须确认所有全松 USB 报告写入端点后，才允许 COMMIT。
RF 参数、IRQ、扫描与矩阵映射均不属于库。状态机的协议尺寸固定，不支持任意数量接收器。
电池模块不采样、不换算百分比；电压由发射端换算。USB 模块内部 HID 状态单实例。

参考接入：`Iemooon/lemon-kbd` 的 `ergo52-nmk/src/{receiver_switch,battery,battery_usb}.rs`。
模块本身不新增存储记录；消费旧版 RMK 的工程升级本分支时，仍须检查已有存储格式变化并备份 Vial。

## Gazell 库分发

可通过独立 build-dependency [`rmk-gazell`](rmk-gazell/README.md) 引用原始 Nordic 库。
它提供库文件与链接路径，不提供 RF 初始化或矩阵驱动。
现有 52840/52833 接收器仍用 SDK 17.1.1；52820 使用 nrfxlib 2.9.0 soft-float，仍需应用 glue。
许可证随库保留，仅适用于 Nordic 芯片，不适用 RMK 的 MIT/Apache 授权。

## BLE 键盘的直接电压传输（2026-11-01）

键盘主从启用 `rmk/battery_voltage`，采样配置仍沿用板级 ADC/分压。
`BatteryProcessor` 额外缓存最终电池 mV，每次采样都发电压事件，充电不会暂停该缓存。
分体从机启用此功能后改发 `BatteryVoltage`，内容是 mV、充电状态及发送时采样年龄，
不再发送百分比 `BatteryStatus`；主机为标准 BLE Battery Service 在本地派生百分比。
旧解码分支保留，但不由新发送端使用；各侧请同步升级，不保证旧固件能解码新消息。

主机需同时启用 `split,custom_message,battery_voltage` 并运行 `ScreenBattery::new(central_is_right)`。
收到 WBAT v2 查询时读取当前采样缓存；每 500ms 最多回复一次。WBAT v1 查询被拒绝。
应用自定义消息上限至少为 16；无需增大 BLE MTU。线格式如下：

| 字节 | 内容 |
| --- | --- |
| 0..3 | `WBAT` |
| 4 / 5 | 版本 2 / 查询 1 或回复 2 |
| 6..9 | u32 LE 查询序号 |
| 10..12 | 左侧 u16 LE 最终 mV + u8 采样年龄秒（向上取整） |
| 13..15 | 右侧同上 |

查询仅 10 字节。电压 0xFFFE=未知，0xFFFF=离线；采样超过 90 秒变未知。
主从重连会清从机缓存；旧缓存重新发送仍携带其真实年龄。

接收器启用 `custom_message,dongle`，运行 `receiver::battery_client::BatteryClient`，
并预留 `custom_message` 和 `dongle_state` 各一个订阅者。
客户端每 30 秒查询，回复须在 1.5 秒内且序号匹配；35 秒无有效回复/断线时清空。
`receiver::battery_client::snapshot()` 返回 EBAT 所需的双侧 `Reading`。
`percentages()` 只供原有屏幕圆圈本地换算，不在无线上传输百分比。

在 `UsbTransport::builder(...).usb_builder()` 中构造一次 `battery_usb::BatteryUsb::new`，
提供 snapshot 和接收器工作状态回调，与客户端/USB 传输并发运行即可。
EBAT 保持 v1，Usage Page FF62 / Usage 52，Report ID 0，32 字节 IN/OUT；
字段 age 表示最近有效回复的接收年龄，WBAT 采样过期由客户端单独处理。
设备切换控制器无需实例化；固定单接收器使用 `(0, true)` 表示 A/工作中。
