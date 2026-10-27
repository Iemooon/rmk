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
